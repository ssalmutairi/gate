use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn setup_test_db() -> PgPool {
    // Clear ADMIN_TOKEN so auth middleware doesn't interfere with tests
    // (test_auth.rs sets it explicitly when needed)
    std::env::remove_var("ADMIN_TOKEN");

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
