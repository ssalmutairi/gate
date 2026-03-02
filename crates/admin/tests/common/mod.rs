use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// The admin token used in tests.
pub const TEST_ADMIN_TOKEN: &str = "test-admin-token";

pub async fn setup_test_db() -> PgPool {
    // Set ADMIN_TOKEN for auth middleware (fail-closed requires a token)
    std::env::set_var("ADMIN_TOKEN", TEST_ADMIN_TOKEN);

    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://gate:gate@localhost:5555/gate_test".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("Failed to connect to test database");

    // Run migrations
    admin::db::run_migrations(&pool).await;

    // Truncate all tables for isolation
    sqlx::query(
        "TRUNCATE TABLE request_logs, header_rules, rate_limits, api_keys, services, routes, targets, upstreams CASCADE"
    )
    .execute(&pool)
    .await
    .expect("Failed to truncate tables");

    pool
}

pub fn build_test_app(pool: PgPool) -> Router {
    admin::build_router(pool)
}

/// Helper: create a GET request with auth token.
pub fn authed_get(uri: &str) -> axum::http::request::Builder {
    axum::http::Request::builder()
        .uri(uri)
        .header("X-Admin-Token", TEST_ADMIN_TOKEN)
}

/// Helper: create a request builder with auth token and a custom method.
pub fn authed_request(method: &str, uri: &str) -> axum::http::request::Builder {
    axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("X-Admin-Token", TEST_ADMIN_TOKEN)
}
