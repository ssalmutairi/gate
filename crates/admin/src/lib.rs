pub mod auth;
pub mod db;
pub mod errors;
pub mod routes;

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};

pub fn build_router(pool: PgPool) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

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
        // Stats & Logs
        .route("/admin/stats", get(routes::stats::get_stats))
        .route("/admin/logs", get(routes::stats::get_logs))
        // Middleware
        .layer(middleware::from_fn(auth::admin_token_middleware))
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(pool)
}
