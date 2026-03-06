use crate::proxy_core::lb::ConnectionTracker;
use crate::proxy_core::router::GatewayConfig;
use arc_swap::ArcSwap;
use shared::models::Target;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

struct TargetHealthState {
    consecutive_failures: u32,
    consecutive_successes: u32,
    healthy: bool,
}

pub async fn run_health_checks(
    pool: &SqlitePool,
    config: Arc<ArcSwap<GatewayConfig>>,
    conn_tracker: Arc<Mutex<ConnectionTracker>>,
    interval_secs: u64,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .expect("Failed to create health check HTTP client");

    let mut states: HashMap<Uuid, TargetHealthState> = HashMap::new();

    let unhealthy_threshold: u32 = 3;
    let healthy_threshold: u32 = 2;

    loop {
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;

        let cfg = config.load();

        let all_targets: Vec<Target> = cfg
            .targets
            .values()
            .flat_map(|targets| targets.iter().cloned())
            .collect();

        let target_ids: std::collections::HashSet<Uuid> =
            all_targets.iter().map(|t| t.id).collect();
        states.retain(|id, _| target_ids.contains(id));

        {
            let ids: Vec<Uuid> = all_targets.iter().map(|t| t.id).collect();
            let mut tracker = conn_tracker.lock().unwrap_or_else(|e| e.into_inner());
            tracker.rebuild(&ids);
        }

        // Run health checks concurrently
        let mut check_futures = Vec::with_capacity(all_targets.len());
        for target in &all_targets {
            let scheme = if target.tls { "https" } else { "http" };
            let url = format!("{}://{}:{}/", scheme, target.host, target.port);
            let client = client.clone();
            let target_id = target.id;
            check_futures.push(async move {
                let result = client.get(&url).send().await;
                let success = matches!(&result, Ok(resp) if resp.status().is_success() || resp.status().is_redirection());
                (target_id, success)
            });
        }

        let results = futures::future::join_all(check_futures).await;

        for (target_id, success) in results {
            let Some(target) = all_targets.iter().find(|t| t.id == target_id) else {
                continue;
            };
            let state = states.entry(target_id).or_insert(TargetHealthState {
                consecutive_failures: 0,
                consecutive_successes: 0,
                healthy: target.healthy,
            });

            if success {
                state.consecutive_successes += 1;
                state.consecutive_failures = 0;

                if !state.healthy && state.consecutive_successes >= healthy_threshold {
                    state.healthy = true;
                    tracing::info!(
                        target_id = %target.id,
                        host = %target.host,
                        port = target.port,
                        "Target recovered — marked healthy"
                    );
                    update_target_health(pool, &target.id, true).await;
                }
            } else {
                state.consecutive_failures += 1;
                state.consecutive_successes = 0;

                if state.healthy && state.consecutive_failures >= unhealthy_threshold {
                    state.healthy = false;
                    tracing::warn!(
                        target_id = %target.id,
                        host = %target.host,
                        port = target.port,
                        failures = state.consecutive_failures,
                        "Target marked unhealthy"
                    );
                    update_target_health(pool, &target.id, false).await;
                }
            }
        }
    }
}

async fn update_target_health(pool: &SqlitePool, target_id: &Uuid, healthy: bool) {
    // Only update the healthy column, not updated_at, to avoid triggering unnecessary config reloads
    let result = sqlx::query("UPDATE targets SET healthy = ?1 WHERE id = ?2")
        .bind(healthy)
        .bind(target_id.to_string())
        .execute(pool)
        .await;

    if let Err(e) = result {
        tracing::error!(target_id = %target_id, error = %e, "Failed to update target health in SQLite");
    }
}
