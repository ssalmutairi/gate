use crate::router::GatewayConfig;
use arc_swap::ArcSwap;
use shared::models::{ApiKey, HeaderRule, IpRule, RateLimit, Route, Target, Upstream};
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

    let ip_rules: Vec<IpRule> = sqlx::query_as("SELECT * FROM ip_rules")
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
        ip_rules = ip_rules.len(),
        "Config loaded from database"
    );

    GatewayConfig::new(routes, upstreams, targets, api_keys, rate_limits, header_rules, ip_rules)
}

#[cfg(test)]
async fn run_test_migrations(pool: &PgPool) {
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
    ];
    for sql in &migrations {
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
                        eprintln!("Migration warning: {}", msg);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
async fn setup_test_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://gate:gate@localhost:5555/gate_test".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("Failed to connect to test database");
    run_test_migrations(&pool).await;
    // Truncate relevant tables
    sqlx::query(
        "TRUNCATE TABLE ip_rules, request_logs, header_rules, rate_limits, api_keys, services, routes, targets, upstreams CASCADE"
    )
    .execute(&pool)
    .await
    .expect("Failed to truncate tables");
    pool
}

/// Runs the config reload loop in a dedicated thread with its own runtime and DB pool.
pub fn spawn_config_reloader(
    database_url: String,
    config: Arc<ArcSwap<GatewayConfig>>,
    interval_secs: u64,
    circuit_breaker: Arc<crate::circuit_breaker::CircuitBreaker>,
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
                            UNION ALL
                            SELECT MAX(updated_at) FROM ip_rules
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

                    // Rebuild circuit breaker configs from upstream settings
                    let mut cb_configs = Vec::new();
                    for (upstream_id, upstream) in &new_config.upstreams {
                        if let Some(threshold) = upstream.circuit_breaker_threshold {
                            if let Some(targets) = new_config.targets.get(upstream_id) {
                                for target in targets {
                                    cb_configs.push((
                                        target.id,
                                        threshold as u32,
                                        upstream.circuit_breaker_duration_secs as u32,
                                    ));
                                }
                            }
                        }
                    }
                    circuit_breaker.rebuild(&cb_configs);

                    config.store(Arc::new(new_config));
                    last_updated = current_max;
                    tracing::info!("Config reloaded from database");
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn load_config_empty_db() {
        let pool = setup_test_pool().await;
        let config = load_config(&pool).await;
        assert!(config.routes.is_empty());
        assert!(config.upstreams.is_empty());
        assert!(config.targets.is_empty());
        assert!(config.api_keys.is_empty());
        assert!(config.rate_limits.is_empty());
        assert!(config.header_rules.is_empty());
    }

    #[tokio::test]
    async fn load_config_with_data() {
        let pool = setup_test_pool().await;

        // Insert upstream
        sqlx::query("INSERT INTO upstreams (id, name, algorithm) VALUES ('a0000000-0000-0000-0000-000000000001', 'cfg-test', 'round_robin')")
            .execute(&pool).await.unwrap();

        // Insert target
        sqlx::query("INSERT INTO targets (upstream_id, host, port) VALUES ('a0000000-0000-0000-0000-000000000001', '127.0.0.1', 8080)")
            .execute(&pool).await.unwrap();

        // Insert route
        sqlx::query("INSERT INTO routes (name, path_prefix, upstream_id, strip_prefix) VALUES ('cfg-route', '/cfg', 'a0000000-0000-0000-0000-000000000001', false)")
            .execute(&pool).await.unwrap();

        let config = load_config(&pool).await;
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.routes[0].path_prefix, "/cfg");
    }

    #[tokio::test]
    async fn load_config_skips_inactive() {
        let pool = setup_test_pool().await;

        // Active upstream
        sqlx::query("INSERT INTO upstreams (id, name, algorithm, active) VALUES ('b0000000-0000-0000-0000-000000000001', 'active-up', 'round_robin', true)")
            .execute(&pool).await.unwrap();

        // Inactive upstream
        sqlx::query("INSERT INTO upstreams (id, name, algorithm, active) VALUES ('b0000000-0000-0000-0000-000000000002', 'inactive-up', 'round_robin', false)")
            .execute(&pool).await.unwrap();

        // Active route
        sqlx::query("INSERT INTO routes (name, path_prefix, upstream_id, strip_prefix, active) VALUES ('active-route', '/active', 'b0000000-0000-0000-0000-000000000001', false, true)")
            .execute(&pool).await.unwrap();

        // Inactive route
        sqlx::query("INSERT INTO routes (name, path_prefix, upstream_id, strip_prefix, active) VALUES ('inactive-route', '/inactive', 'b0000000-0000-0000-0000-000000000001', false, false)")
            .execute(&pool).await.unwrap();

        let config = load_config(&pool).await;
        // Only active routes and upstreams loaded
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0].name, "active-route");
        assert_eq!(config.upstreams.len(), 1);
    }

    #[tokio::test]
    async fn load_config_loads_api_keys_and_rate_limits() {
        let pool = setup_test_pool().await;

        // Insert upstream and route
        sqlx::query("INSERT INTO upstreams (id, name, algorithm) VALUES ('c0000000-0000-0000-0000-000000000001', 'auth-up', 'round_robin')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO routes (id, name, path_prefix, upstream_id, strip_prefix) VALUES ('c0000000-0000-0000-0000-000000000002', 'auth-route', '/auth', 'c0000000-0000-0000-0000-000000000001', false)")
            .execute(&pool).await.unwrap();

        // Insert API key
        sqlx::query("INSERT INTO api_keys (name, key_hash, route_id) VALUES ('test-key', 'abc123hash', 'c0000000-0000-0000-0000-000000000002')")
            .execute(&pool).await.unwrap();

        // Insert rate limit
        sqlx::query("INSERT INTO rate_limits (route_id, requests_per_second, limit_by) VALUES ('c0000000-0000-0000-0000-000000000002', 100, 'ip')")
            .execute(&pool).await.unwrap();

        // Insert header rule
        sqlx::query("INSERT INTO header_rules (route_id, phase, action, header_name, header_value) VALUES ('c0000000-0000-0000-0000-000000000002', 'request', 'set', 'X-Custom', 'value')")
            .execute(&pool).await.unwrap();

        let config = load_config(&pool).await;
        assert_eq!(config.api_keys.len(), 1);
        assert_eq!(config.api_keys[0].key_hash, "abc123hash");
        assert_eq!(config.rate_limits.len(), 1);
        let route_id: uuid::Uuid = "c0000000-0000-0000-0000-000000000002".parse().unwrap();
        assert_eq!(config.rate_limits[&route_id].requests_per_second, 100);
        assert_eq!(config.header_rules.len(), 1);
        assert_eq!(config.header_rules[&route_id].len(), 1);
    }
}
