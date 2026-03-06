use crate::config_loader::{load_config, collect_cb_configs};
use crate::proxy_core::router::GatewayConfig;
use arc_swap::ArcSwap;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;

pub fn spawn_config_reloader(
    pool: SqlitePool,
    config: Arc<ArcSwap<GatewayConfig>>,
    interval_secs: u64,
    state: Arc<crate::proxy_core::state::StateBackend>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build reloader runtime");

        rt.block_on(async move {
            let mut last_updated: Option<String> = None;

            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;

                let max_updated: Option<(Option<String>,)> = sqlx::query_as(
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

                    let cb_configs = collect_cb_configs(&new_config);
                    state.circuit_breaker().rebuild(&cb_configs);

                    config.store(Arc::new(new_config));
                    last_updated = current_max;
                    tracing::info!("Config reloaded from SQLite");
                }
            }
        });
    });
}
