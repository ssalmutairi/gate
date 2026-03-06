use arc_swap::ArcSwap;
use clap::Parser;
use proxy_core::lb::ConnectionTracker;
use proxy_core::service::{GatewayProxy, RequestLogEntry, LOG_CHANNEL_CAPACITY};
use pingora::prelude::*;
use std::sync::{Arc, Mutex};

mod auth;
mod config_loader;
mod config_reloader;
mod dashboard;
mod db;
mod errors;
mod gateway_proxy;
mod health_checker;
mod models;
mod proxy_core;
mod request_stats;
mod routes;
mod wsdl;

use shared::config::AppConfig;

#[derive(Parser)]
#[command(name = "gate-portable", version, about = "Gate API Gateway — portable mode (single binary, zero dependencies)")]
struct Cli {
    /// Admin API port
    #[arg(long, short = 'a')]
    admin_port: Option<u16>,

    /// Proxy port
    #[arg(long, short = 'p')]
    proxy_port: Option<u16>,

    /// Metrics port
    #[arg(long, short = 'm')]
    metrics_port: Option<u16>,

    /// SQLite database path
    #[arg(long, short = 'd')]
    db: Option<String>,

    /// Admin token
    #[arg(long, short = 't')]
    token: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, short = 'l')]
    log_level: Option<String>,
}

fn main() {
    // Install rustls crypto provider for TLS upstream connections
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = Cli::parse();

    // CLI args override env vars; standalone defaults apply last
    if let Some(v) = cli.admin_port { std::env::set_var("ADMIN_PORT", v.to_string()); }
    if let Some(v) = cli.proxy_port { std::env::set_var("PROXY_PORT", v.to_string()); }
    if let Some(v) = cli.metrics_port { std::env::set_var("METRICS_PORT", v.to_string()); }
    if let Some(v) = cli.db { std::env::set_var("DATABASE_URL", v); }
    if let Some(v) = cli.token { std::env::set_var("ADMIN_TOKEN", v); }
    if let Some(v) = cli.log_level { std::env::set_var("LOG_LEVEL", v); }

    // Standalone defaults (only if not set by CLI or env)
    if std::env::var("DATABASE_URL").is_err() {
        std::env::set_var("DATABASE_URL", "sqlite://gate.db");
    }
    if std::env::var("ADMIN_TOKEN").is_err() {
        std::env::set_var("ADMIN_TOKEN", "changeme");
    }

    let app_config = AppConfig::from_env();

    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&app_config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    print_banner(&app_config);

    // Create a Tokio runtime for async DB setup
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime for setup");

    // Initialize SQLite pool, run migrations, and load initial config
    let (pool, gateway_config) = rt.block_on(async {
        let pool = db::create_pool(&app_config.database_url).await;
        db::run_migrations(&pool).await;
        tracing::info!("SQLite database ready");

        let cfg = config_loader::load_config(&pool).await;
        (pool, Arc::new(ArcSwap::from_pointee(cfg)))
    });

    // Shared connection tracker for least-connections algorithm
    let conn_tracker = Arc::new(Mutex::new(ConnectionTracker::new()));

    // Build state backend (always in-memory for standalone)
    let state_backend: Arc<proxy_core::state::StateBackend> = {
        let circuit_breaker = Arc::new(proxy_core::circuit_breaker::CircuitBreaker::new());

        // Configure circuit breakers from initial config
        {
            let cfg = gateway_config.load();
            for (target_id, threshold, duration) in config_loader::collect_cb_configs(&cfg) {
                circuit_breaker.configure(target_id, threshold, duration);
            }
        }

        Arc::new(proxy_core::state::StateBackend::Memory(
            proxy_core::state::MemoryState::new(circuit_breaker),
        ))
    };

    proxy_core::metrics::STATE_BACKEND_REDIS.set(0.0);
    tracing::info!("State backend: In-Memory (standalone)");

    // Spawn config reloader
    config_reloader::spawn_config_reloader(
        pool.clone(),
        gateway_config.clone(),
        app_config.config_poll_interval_secs,
        state_backend.clone(),
    );

    // Spawn health checker
    let health_config = gateway_config.clone();
    let health_tracker = conn_tracker.clone();
    let health_pool = pool.clone();
    let health_interval = app_config.health_check_interval_secs;
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build health check runtime");

        rt.block_on(async move {
            health_checker::run_health_checks(&health_pool, health_config, health_tracker, health_interval).await;
        });
    });

    // In-memory request stats — standalone counts requests without persisting
    let stats = Arc::new(request_stats::RequestStats::new());
    let (log_sender, mut log_receiver) = tokio::sync::mpsc::channel::<RequestLogEntry>(LOG_CHANNEL_CAPACITY);
    let stats_writer = stats.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            while let Some(entry) = log_receiver.recv().await {
                stats_writer.record(entry.status_code, entry.latency_ms);
            }
        });
    });

    // Spawn admin API (Axum) in a background thread
    let admin_pool = pool;
    let admin_port = app_config.admin_port;
    let max_spec_size_bytes = app_config.max_spec_size_mb * 1024 * 1024;
    let admin_stats = stats.clone();
    let admin_config = gateway_config.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build admin API runtime");

        rt.block_on(async move {
            let router = build_admin_router(admin_pool, max_spec_size_bytes, admin_stats, admin_config);
            let addr = format!("0.0.0.0:{}", admin_port);
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("Failed to bind admin server");
            tracing::info!(addr = %addr, "Admin API listening");
            axum::serve(listener, router)
                .await
                .expect("Admin server failed");
        });
    });

    // Initialize Prometheus metrics and start metrics server
    proxy_core::metrics::init();
    proxy_core::metrics::spawn_metrics_server(app_config.metrics_port);

    // Create Pingora server
    let mut server = Server::new(None).expect("Failed to create Pingora server");
    server.bootstrap();

    // Create the proxy service
    let proxy = GatewayProxy::new(
        gateway_config,
        conn_tracker,
        log_sender,
        app_config.trusted_proxies.clone(),
        state_backend,
    );
    let mut proxy_service = http_proxy_service(&server.configuration, proxy);

    let addr = format!("0.0.0.0:{}", app_config.proxy_port);
    proxy_service.add_tcp(&addr);
    tracing::info!(addr = %addr, "Proxy listening");

    server.add_service(proxy_service);
    server.run_forever();
}

fn build_admin_router(
    pool: sqlx::SqlitePool,
    max_spec_size_bytes: usize,
    stats: Arc<request_stats::RequestStats>,
    gateway_config: Arc<ArcSwap<proxy_core::router::GatewayConfig>>,
) -> axum::Router {
    use axum::extract::DefaultBodyLimit;
    use axum::middleware;
    use axum::routing::{delete, get, post, put};
    use axum::Extension;
    use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

    let cors = match std::env::var("CORS_ALLOWED_ORIGINS") {
        Ok(origins) if !origins.is_empty() && origins != "*" => {
            let allowed: Vec<_> = origins
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(allowed)
                .allow_methods(AllowMethods::mirror_request())
                .allow_headers(AllowHeaders::mirror_request())
        }
        _ => CorsLayer::new()
            .allow_origin(AllowOrigin::mirror_request())
            .allow_methods(AllowMethods::mirror_request())
            .allow_headers(AllowHeaders::mirror_request()),
    };

    axum::Router::new()
        // Health (no auth)
        .route("/admin/health", get(routes::health::health_check))
        // Stats & Logs (standalone returns empty/zeroed — no request logging)
        .route("/admin/stats", get(routes::health::stats))
        .route("/admin/logs", get(routes::health::logs))
        // Routes
        .route("/admin/routes", get(routes::routes::list_routes))
        .route("/admin/routes", post(routes::routes::create_route))
        .route(
            "/admin/routes/:id",
            get(routes::routes::get_route)
                .put(routes::routes::update_route)
                .delete(routes::routes::delete_route),
        )
        // Upstreams
        .route("/admin/upstreams", get(routes::upstreams::list_upstreams))
        .route(
            "/admin/upstreams",
            post(routes::upstreams::create_upstream),
        )
        .route(
            "/admin/upstreams/:id",
            get(routes::upstreams::get_upstream)
                .put(routes::upstreams::update_upstream)
                .delete(routes::upstreams::delete_upstream),
        )
        // Targets
        .route(
            "/admin/upstreams/:id/targets",
            post(routes::upstreams::add_target),
        )
        .route(
            "/admin/upstreams/:id/targets/:target_id",
            delete(routes::upstreams::delete_target),
        )
        // API Keys
        .route("/admin/api-keys", get(routes::api_keys::list_api_keys))
        .route("/admin/api-keys", post(routes::api_keys::create_api_key))
        .route(
            "/admin/api-keys/:id",
            put(routes::api_keys::update_api_key)
                .delete(routes::api_keys::delete_api_key),
        )
        // Rate Limits
        .route(
            "/admin/rate-limits",
            get(routes::rate_limits::list_rate_limits),
        )
        .route(
            "/admin/rate-limits",
            post(routes::rate_limits::create_rate_limit),
        )
        .route(
            "/admin/rate-limits/:id",
            put(routes::rate_limits::update_rate_limit)
                .delete(routes::rate_limits::delete_rate_limit),
        )
        // Header Rules
        .route(
            "/admin/routes/:route_id/header-rules",
            get(routes::header_rules::list_header_rules)
                .post(routes::header_rules::create_header_rule),
        )
        .route(
            "/admin/header-rules/:id",
            delete(routes::header_rules::delete_header_rule),
        )
        // IP Rules
        .route(
            "/admin/routes/:route_id/ip-rules",
            get(routes::ip_rules::list_ip_rules)
                .post(routes::ip_rules::create_ip_rule),
        )
        .route(
            "/admin/ip-rules/:id",
            delete(routes::ip_rules::delete_ip_rule),
        )
        // Services
        .route(
            "/admin/services/import",
            post(routes::services::import_service),
        )
        .route("/admin/services", get(routes::services::list_services))
        .route(
            "/admin/services/:id",
            get(routes::services::get_service)
                .put(routes::services::update_service)
                .delete(routes::services::delete_service),
        )
        .route(
            "/admin/services/:id/spec",
            get(routes::services::get_service_spec),
        )
        // Middleware
        .layer(middleware::from_fn(auth::admin_token_middleware))
        .layer(cors)
        .layer(DefaultBodyLimit::max(max_spec_size_bytes))
        .layer(Extension(routes::services::AppSettings { max_spec_size_bytes }))
        .layer(Extension(stats))
        .layer(Extension(gateway_config))
        // Gateway reverse proxy (Try It panel in dashboard)
        .route("/gateway/*rest", axum::routing::any(gateway_proxy::proxy_to_gateway))
        .with_state(pool)
        .fallback(dashboard::dashboard_handler)
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    async fn setup() -> (SqlitePool, axum::Router) {
        std::env::set_var("ADMIN_TOKEN", "test-token");
        let pool = db::create_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let stats = Arc::new(request_stats::RequestStats::new());
        let gateway_config = Arc::new(ArcSwap::from_pointee(
            config_loader::load_config(&pool).await,
        ));
        let router = build_admin_router(pool.clone(), 10 * 1024 * 1024, stats, gateway_config);
        (pool, router)
    }

    fn auth_req(method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("X-Admin-Token", "test-token")
            .header("Content-Type", "application/json");
        match body {
            Some(b) => builder.body(Body::from(serde_json::to_vec(&b).unwrap())).unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // --- Health ---

    #[tokio::test]
    async fn health_no_auth() {
        let (_pool, app) = setup().await;
        let req = Request::builder().uri("/admin/health").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_required() {
        let (_pool, app) = setup().await;
        let req = Request::builder()
            .uri("/admin/upstreams")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // --- Upstreams CRUD ---

    #[tokio::test]
    async fn upstream_crud() {
        let (_pool, app) = setup().await;

        // Create upstream
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "test-upstream"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let upstream_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["name"], "test-upstream");
        assert_eq!(body["algorithm"], "round_robin");
        assert_eq!(body["active"], true);

        // Get upstream
        let resp = app.clone().oneshot(auth_req("GET", &format!("/admin/upstreams/{}", upstream_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["name"], "test-upstream");

        // Update upstream
        let resp = app.clone().oneshot(auth_req("PUT", &format!("/admin/upstreams/{}", upstream_id), Some(serde_json::json!({
            "name": "updated-upstream",
            "algorithm": "least_connections"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["name"], "updated-upstream");
        assert_eq!(body["algorithm"], "least_connections");

        // List upstreams
        let resp = app.clone().oneshot(auth_req("GET", "/admin/upstreams", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["total"], 1);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);

        // Delete upstream
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/upstreams/{}", upstream_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify deleted
        let resp = app.oneshot(auth_req("GET", &format!("/admin/upstreams/{}", upstream_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn upstream_validation() {
        let (_pool, app) = setup().await;

        // Empty name
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": ""
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Invalid algorithm
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "test",
            "algorithm": "invalid"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // --- Targets ---

    #[tokio::test]
    async fn target_crud() {
        let (_pool, app) = setup().await;

        // Create upstream first
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "target-test-upstream"
        })))).await.unwrap();
        let body = body_json(resp).await;
        let upstream_id = body["id"].as_str().unwrap().to_string();

        // Add target
        let resp = app.clone().oneshot(auth_req("POST", &format!("/admin/upstreams/{}/targets", upstream_id), Some(serde_json::json!({
            "host": "localhost",
            "port": 8080
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let target_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["host"], "localhost");
        assert_eq!(body["port"], 8080);

        // Verify target shows in upstream
        let resp = app.clone().oneshot(auth_req("GET", &format!("/admin/upstreams/{}", upstream_id), None)).await.unwrap();
        let body = body_json(resp).await;
        assert_eq!(body["targets"].as_array().unwrap().len(), 1);

        // Delete target
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/upstreams/{}/targets/{}", upstream_id, target_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn target_validation() {
        let (_pool, app) = setup().await;

        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "target-val-upstream"
        })))).await.unwrap();
        let body = body_json(resp).await;
        let upstream_id = body["id"].as_str().unwrap().to_string();

        // Invalid port
        let resp = app.clone().oneshot(auth_req("POST", &format!("/admin/upstreams/{}/targets", upstream_id), Some(serde_json::json!({
            "host": "localhost",
            "port": 0
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Non-existent upstream
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams/00000000-0000-0000-0000-000000000000/targets", Some(serde_json::json!({
            "host": "localhost",
            "port": 8080
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // --- Routes CRUD ---

    #[tokio::test]
    async fn route_crud() {
        let (_pool, app) = setup().await;

        // Create upstream first
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "route-test-upstream"
        })))).await.unwrap();
        let body = body_json(resp).await;
        let upstream_id = body["id"].as_str().unwrap().to_string();

        // Create route
        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "test-route",
            "path_prefix": "/api/v1",
            "upstream_id": upstream_id,
            "methods": ["GET", "POST"]
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let route_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["name"], "test-route");
        assert_eq!(body["path_prefix"], "/api/v1");
        assert_eq!(body["methods"], serde_json::json!(["GET", "POST"]));

        // Get route
        let resp = app.clone().oneshot(auth_req("GET", &format!("/admin/routes/{}", route_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["upstream_name"], "route-test-upstream");

        // Update route
        let resp = app.clone().oneshot(auth_req("PUT", &format!("/admin/routes/{}", route_id), Some(serde_json::json!({
            "name": "updated-route",
            "strip_prefix": true
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["name"], "updated-route");
        assert_eq!(body["strip_prefix"], true);

        // List routes
        let resp = app.clone().oneshot(auth_req("GET", "/admin/routes", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["total"], 1);

        // Delete route
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/routes/{}", route_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn route_validation() {
        let (_pool, app) = setup().await;

        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "route-val-upstream"
        })))).await.unwrap();
        let body = body_json(resp).await;
        let upstream_id = body["id"].as_str().unwrap().to_string();

        // Missing path prefix slash
        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "bad-route",
            "path_prefix": "no-slash",
            "upstream_id": upstream_id
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Invalid retries
        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "bad-route",
            "path_prefix": "/api",
            "upstream_id": upstream_id,
            "retries": 10
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // --- API Keys ---

    #[tokio::test]
    async fn api_key_crud() {
        let (_pool, app) = setup().await;

        // Create API key (no route_id)
        let resp = app.clone().oneshot(auth_req("POST", "/admin/api-keys", Some(serde_json::json!({
            "name": "test-key"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let key_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["name"], "test-key");
        assert!(body["key"].as_str().unwrap().starts_with("gw_"));

        // List API keys
        let resp = app.clone().oneshot(auth_req("GET", "/admin/api-keys", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["total"], 1);

        // Update API key
        let resp = app.clone().oneshot(auth_req("PUT", &format!("/admin/api-keys/{}", key_id), Some(serde_json::json!({
            "active": false
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["active"], false);

        // Delete API key
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/api-keys/{}", key_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // --- Rate Limits ---

    #[tokio::test]
    async fn rate_limit_crud() {
        let (_pool, app) = setup().await;

        // Create upstream and route
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "rl-upstream"
        })))).await.unwrap();
        let upstream_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "rl-route",
            "path_prefix": "/api",
            "upstream_id": upstream_id
        })))).await.unwrap();
        let route_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        // Create rate limit
        let resp = app.clone().oneshot(auth_req("POST", "/admin/rate-limits", Some(serde_json::json!({
            "route_id": route_id,
            "requests_per_second": 100,
            "limit_by": "ip"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let rl_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["requests_per_second"], 100);

        // List rate limits
        let resp = app.clone().oneshot(auth_req("GET", "/admin/rate-limits", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["total"], 1);

        // Update rate limit
        let resp = app.clone().oneshot(auth_req("PUT", &format!("/admin/rate-limits/{}", rl_id), Some(serde_json::json!({
            "requests_per_second": 200
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["requests_per_second"], 200);

        // Delete rate limit
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/rate-limits/{}", rl_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // --- Header Rules ---

    #[tokio::test]
    async fn header_rule_crud() {
        let (_pool, app) = setup().await;

        // Create upstream and route
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "hr-upstream"
        })))).await.unwrap();
        let upstream_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "hr-route",
            "path_prefix": "/api",
            "upstream_id": upstream_id
        })))).await.unwrap();
        let route_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        // Create header rule
        let resp = app.clone().oneshot(auth_req("POST", &format!("/admin/routes/{}/header-rules", route_id), Some(serde_json::json!({
            "phase": "request",
            "action": "set",
            "header_name": "X-Custom",
            "header_value": "hello"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let hr_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["header_name"], "X-Custom");

        // List header rules
        let resp = app.clone().oneshot(auth_req("GET", &format!("/admin/routes/{}/header-rules", route_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 1);

        // Delete header rule
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/header-rules/{}", hr_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // --- IP Rules ---

    #[tokio::test]
    async fn ip_rule_crud() {
        let (_pool, app) = setup().await;

        // Create upstream and route
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "ip-upstream"
        })))).await.unwrap();
        let upstream_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "ip-route",
            "path_prefix": "/api",
            "upstream_id": upstream_id
        })))).await.unwrap();
        let route_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        // Create IP rule
        let resp = app.clone().oneshot(auth_req("POST", &format!("/admin/routes/{}/ip-rules", route_id), Some(serde_json::json!({
            "cidr": "192.168.1.0/24",
            "action": "allow",
            "description": "internal network"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        let ip_id = body["id"].as_str().unwrap().to_string();
        assert_eq!(body["cidr"], "192.168.1.0/24");

        // List IP rules
        let resp = app.clone().oneshot(auth_req("GET", &format!("/admin/routes/{}/ip-rules", route_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 1);

        // Delete IP rule
        let resp = app.clone().oneshot(auth_req("DELETE", &format!("/admin/ip-rules/{}", ip_id), None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // --- Pagination ---

    #[tokio::test]
    async fn pagination() {
        let (_pool, app) = setup().await;

        // Create 3 upstreams
        for i in 0..3 {
            app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
                "name": format!("upstream-{}", i)
            })))).await.unwrap();
        }

        // Page 1 with limit 2
        let resp = app.clone().oneshot(auth_req("GET", "/admin/upstreams?page=1&limit=2", None)).await.unwrap();
        let body = body_json(resp).await;
        assert_eq!(body["total"], 3);
        assert_eq!(body["data"].as_array().unwrap().len(), 2);
        assert_eq!(body["page"], 1);
        assert_eq!(body["limit"], 2);

        // Page 2
        let resp = app.clone().oneshot(auth_req("GET", "/admin/upstreams?page=2&limit=2", None)).await.unwrap();
        let body = body_json(resp).await;
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["page"], 2);
    }

    // --- Duplicate name conflict ---

    #[tokio::test]
    async fn upstream_duplicate_name() {
        let (_pool, app) = setup().await;

        app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "dup-upstream"
        })))).await.unwrap();

        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "dup-upstream"
        })))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // --- Not found ---

    #[tokio::test]
    async fn not_found_returns_404() {
        let (_pool, app) = setup().await;

        let resp = app.clone().oneshot(auth_req("GET", "/admin/upstreams/00000000-0000-0000-0000-000000000000", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let resp = app.clone().oneshot(auth_req("GET", "/admin/routes/00000000-0000-0000-0000-000000000000", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // --- Config loader ---

    #[tokio::test]
    async fn config_loader_loads_active_only() {
        let (pool, app) = setup().await;

        // Create active upstream with target
        let resp = app.clone().oneshot(auth_req("POST", "/admin/upstreams", Some(serde_json::json!({
            "name": "active-upstream"
        })))).await.unwrap();
        let upstream_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        app.clone().oneshot(auth_req("POST", &format!("/admin/upstreams/{}/targets", upstream_id), Some(serde_json::json!({
            "host": "localhost",
            "port": 8080
        })))).await.unwrap();

        // Create route
        let resp = app.clone().oneshot(auth_req("POST", "/admin/routes", Some(serde_json::json!({
            "name": "active-route",
            "path_prefix": "/test",
            "upstream_id": upstream_id
        })))).await.unwrap();
        let route_id = body_json(resp).await["id"].as_str().unwrap().to_string();

        // Deactivate the upstream
        app.clone().oneshot(auth_req("PUT", &format!("/admin/upstreams/{}", upstream_id), Some(serde_json::json!({
            "active": false
        })))).await.unwrap();

        // Load config — should have 0 active upstreams and 0 targets
        let config = config_loader::load_config(&pool).await;
        assert_eq!(config.upstreams.len(), 0);
        // Targets should be empty since upstream is inactive
        let total_targets: usize = config.targets.values().map(|t| t.len()).sum();
        assert_eq!(total_targets, 0);

        // But route still exists (routes table has its own active flag)
        // Deactivate route too
        app.clone().oneshot(auth_req("PUT", &format!("/admin/routes/{}", route_id), Some(serde_json::json!({
            "active": false
        })))).await.unwrap();

        let config = config_loader::load_config(&pool).await;
        assert_eq!(config.routes.len(), 0);
    }
}

fn print_banner(config: &AppConfig) {
    let proxy_addr = format!("0.0.0.0:{}", config.proxy_port);
    let admin_addr = format!("0.0.0.0:{}", config.admin_port);
    let metrics_addr = format!("0.0.0.0:{}", config.metrics_port);
    let reload = format!("every {}s", config.config_poll_interval_secs);
    let health = format!("every {}s", config.health_check_interval_secs);
    eprintln!();
    eprintln!("  ┌─────────────────────────────────────────┐");
    eprintln!("  │   Gate Portable v{}            │", env!("CARGO_PKG_VERSION"));
    eprintln!("  ├─────────────────────────────────────────┤");
    eprintln!("  │  Proxy:   {:<30}│", proxy_addr);
    eprintln!("  │  Admin:   {:<30}│", admin_addr);
    eprintln!("  │  Metrics: {:<30}│", metrics_addr);
    eprintln!("  │  Reload:  {:<30}│", reload);
    eprintln!("  │  Health:  {:<30}│", health);
    eprintln!("  │  State:   {:<30}│", "In-Memory (SQLite)");
    eprintln!("  │  DB:      {:<30}│", "SQLite (embedded)");
    eprintln!("  └─────────────────────────────────────────┘");
    eprintln!();
}
