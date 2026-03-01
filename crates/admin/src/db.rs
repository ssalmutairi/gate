use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
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
