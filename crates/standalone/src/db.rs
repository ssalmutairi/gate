use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn create_pool(database_url: &str) -> SqlitePool {
    let opts = SqliteConnectOptions::from_str(database_url)
        .expect("Invalid DATABASE_URL")
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
        .expect("Failed to connect to SQLite database");

    pool
}

pub async fn run_migrations(pool: &SqlitePool) {
    let sql = include_str!("../migrations/001_init.sql");
    for statement in sql.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        match sqlx::query(trimmed).execute(pool).await {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("already exists") {
                    tracing::warn!("Migration statement warning: {}", msg);
                }
            }
        }
    }
    tracing::info!("SQLite migrations applied");
}
