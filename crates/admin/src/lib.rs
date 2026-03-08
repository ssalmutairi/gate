pub mod auth;
pub mod dashboard;
pub mod db;
pub mod errors;
pub mod gateway_proxy;
pub mod routes;
pub mod wsdl;

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Extension;
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

/// Shared config values available to handlers via Extension.
#[derive(Clone)]
pub struct AppSettings {
    pub max_spec_size_bytes: usize,
}

pub fn build_router(pool: PgPool) -> Router {
    build_router_with_config(pool, 25 * 1024 * 1024)
}

pub fn build_router_with_config(pool: PgPool, max_spec_size_bytes: usize) -> Router {
    // CORS: use CORS_ALLOWED_ORIGINS env var if set, otherwise allow same-origin only
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
        _ => {
            // Default: allow all origins (needed when dashboard is embedded in same binary)
            CorsLayer::new()
                .allow_origin(AllowOrigin::mirror_request())
                .allow_methods(AllowMethods::mirror_request())
                .allow_headers(AllowHeaders::mirror_request())
        }
    };

    Router::new()
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
        // Compositions
        .route(
            "/admin/compositions",
            get(routes::compositions::list_compositions)
                .post(routes::compositions::create_composition),
        )
        .route(
            "/admin/compositions/namespaces",
            get(routes::compositions::list_namespaces),
        )
        .route(
            "/admin/compositions/namespaces/:ns/openapi",
            get(routes::compositions::get_namespace_openapi),
        )
        .route(
            "/admin/compositions/:id",
            get(routes::compositions::get_composition)
                .put(routes::compositions::update_composition)
                .delete(routes::compositions::delete_composition),
        )
        .route(
            "/admin/compositions/:id/openapi",
            get(routes::compositions::get_composition_openapi),
        )
        // Stats & Logs
        .route("/admin/stats", get(routes::stats::get_stats))
        .route("/admin/logs", get(routes::stats::get_logs))
        // Middleware
        .layer(middleware::from_fn(auth::admin_token_middleware))
        .layer(cors)
        .layer(DefaultBodyLimit::max(max_spec_size_bytes))
        .layer(Extension(AppSettings { max_spec_size_bytes }))
        // Gateway reverse proxy (Try It panel in dashboard)
        .route("/gateway/*rest", axum::routing::any(gateway_proxy::proxy_to_gateway))
        .with_state(pool)
        .fallback(dashboard::dashboard_handler)
}
