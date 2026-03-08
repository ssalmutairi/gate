use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await
        .expect("Failed to connect to database")
}

pub async fn run_migrations(pool: &PgPool) {
    let migrations = [
        include_str!("../../../migrations/001_create_upstreams.sql"),
        include_str!("../../../migrations/002_create_targets.sql"),
        include_str!("../../../migrations/003_create_routes.sql"),
        include_str!("../../../migrations/004_create_api_keys.sql"),
        include_str!("../../../migrations/005_create_rate_limits.sql"),
        include_str!("../../../migrations/006_create_request_logs.sql"),
        include_str!("../../../migrations/007_create_services.sql"),
        include_str!("../../../migrations/008_add_route_max_body.sql"),
        include_str!("../../../migrations/009_add_spec_content.sql"),
        include_str!("../../../migrations/010_add_route_auth_skip.sql"),
        include_str!("../../../migrations/011_add_resilience.sql"),
        include_str!("../../../migrations/012_add_route_host_and_cache.sql"),
        include_str!("../../../migrations/013_create_ip_rules.sql"),
        include_str!("../../../migrations/014_add_soap_support.sql"),
        include_str!("../../../migrations/015_create_compositions.sql"),
    ];

    for (i, sql) in migrations.iter().enumerate() {
        // Split by statements and execute, ignoring "already exists" errors
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            match sqlx::query(trimmed).execute(pool).await {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("already exists") {
                        continue;
                    }
                    tracing::warn!(
                        migration = i + 1,
                        error = %msg,
                        "Migration statement warning"
                    );
                }
            }
        }
    }
    tracing::info!("Database migrations applied");
}
