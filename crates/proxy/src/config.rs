use crate::router::GatewayConfig;
use arc_swap::ArcSwap;
use shared::models::{ApiKey, HeaderRule, RateLimit, Route, Target, Upstream};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

/// Loads all config from the database.
pub async fn load_config(pool: &PgPool) -> GatewayConfig {
    let routes: Vec<Route> = sqlx::query_as("SELECT * FROM routes WHERE active = true")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let upstreams: Vec<Upstream> = sqlx::query_as("SELECT * FROM upstreams WHERE active = true")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let targets: Vec<Target> = sqlx::query_as("SELECT * FROM targets")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let api_keys: Vec<ApiKey> = sqlx::query_as("SELECT * FROM api_keys")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let rate_limits: Vec<RateLimit> = sqlx::query_as("SELECT * FROM rate_limits")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let header_rules: Vec<HeaderRule> = sqlx::query_as("SELECT * FROM header_rules")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    tracing::info!(
        routes = routes.len(),
        upstreams = upstreams.len(),
        targets = targets.len(),
        api_keys = api_keys.len(),
        rate_limits = rate_limits.len(),
        header_rules = header_rules.len(),
        "Config loaded from database"
    );

    GatewayConfig::new(routes, upstreams, targets, api_keys, rate_limits, header_rules)
}

/// Runs the config reload loop in a dedicated thread with its own runtime and DB pool.
pub fn spawn_config_reloader(
    database_url: String,
    config: Arc<ArcSwap<GatewayConfig>>,
    interval_secs: u64,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build reloader runtime");

        rt.block_on(async move {
            let pool = PgPoolOptions::new()
                .max_connections(2)
                .connect(&database_url)
                .await
                .expect("Reloader failed to connect to database");

            let mut last_updated: Option<chrono::DateTime<chrono::Utc>> = None;

            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;

                let max_updated: Option<(Option<chrono::DateTime<chrono::Utc>>,)> =
                    sqlx::query_as(
                        r#"SELECT MAX(latest) FROM (
                            SELECT MAX(updated_at) as latest FROM routes
                            UNION ALL
                            SELECT MAX(updated_at) FROM upstreams
                            UNION ALL
                            SELECT MAX(updated_at) FROM targets
                            UNION ALL
                            SELECT MAX(updated_at) FROM api_keys
                            UNION ALL
                            SELECT MAX(updated_at) FROM rate_limits
                            UNION ALL
                            SELECT MAX(updated_at) FROM header_rules
                        ) sub"#,
                    )
                    .fetch_optional(&pool)
                    .await
                    .ok()
                    .flatten();

                let current_max = max_updated.and_then(|r| r.0);

                let should_reload = match (&last_updated, &current_max) {
                    (None, Some(_)) => true,
                    (Some(last), Some(current)) => current > last,
                    _ => false,
                };

                if should_reload {
                    let new_config = load_config(&pool).await;
                    config.store(Arc::new(new_config));
                    last_updated = current_max;
                    tracing::info!("Config reloaded from database");
                }
            }
        });
    });
}
