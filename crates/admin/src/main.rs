use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

mod auth;
mod db;
mod errors;
mod routes;

use shared::config::AppConfig;

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let bind = format!("{}:{}", config.admin_bind_addr, config.admin_port);
    let auth = if config.admin_token.is_some() { "token required" } else { "open (no token)" };
    eprintln!();
    eprintln!("  ┌───────────────────────────────────┐");
    eprintln!("  │      Gate Admin API v1.0.0        │");
    eprintln!("  ├───────────────────────────────────┤");
    eprintln!("  │  Bind: {:<27}│", bind);
    eprintln!("  │  Auth: {:<27}│", auth);
    eprintln!("  └───────────────────────────────────┘");
    eprintln!();

    tracing::info!(admin_port = config.admin_port, "Starting Gate admin API");

    let pool = db::create_pool(&config.database_url).await;
    db::run_migrations(&pool).await;

    // CORS: allow dashboard origins
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health (no auth)
        .route("/admin/health", get(routes::health::health_check))
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
        .route("/admin/upstreams", post(routes::upstreams::create_upstream))
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
        // Stats & Logs
        .route("/admin/stats", get(routes::stats::get_stats))
        .route("/admin/logs", get(routes::stats::get_logs))
        // Middleware
        .layer(middleware::from_fn(auth::admin_token_middleware))
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 1024)) // 1MB max body
        .with_state(pool);

    let addr = format!("{}:{}", config.admin_bind_addr, config.admin_port);
    tracing::info!(addr = %addr, "Admin API listening");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, stopping admin API...");
}
