use crate::lb::ConnectionTracker;
use crate::router::GatewayConfig;
use arc_swap::ArcSwap;
use shared::models::Target;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

/// Health state for a single target.
struct TargetHealthState {
    consecutive_failures: u32,
    consecutive_successes: u32,
    healthy: bool,
}

/// Runs health checks for all upstream targets.
pub async fn run_health_checks(
    pool: &PgPool,
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

    // Thresholds
    let unhealthy_threshold: u32 = 3; // 3 consecutive failures → unhealthy
    let healthy_threshold: u32 = 2; // 2 consecutive successes → healthy

    loop {
        tokio::time::sleep(Duration::from_secs(interval_secs)).await;

        let cfg = config.load();

        // Collect all targets to check
        let all_targets: Vec<Target> = cfg
            .targets
            .values()
            .flat_map(|targets| targets.iter().cloned())
            .collect();

        // Clean up states for targets that no longer exist
        let target_ids: std::collections::HashSet<Uuid> =
            all_targets.iter().map(|t| t.id).collect();
        states.retain(|id, _| target_ids.contains(id));

        // Rebuild connection tracker with current target IDs
        {
            let ids: Vec<Uuid> = all_targets.iter().map(|t| t.id).collect();
            let mut tracker = conn_tracker.lock().unwrap();
            tracker.rebuild(&ids);
        }

        for target in &all_targets {
            let scheme = if target.tls { "https" } else { "http" };
            let url = format!("{}://{}:{}/", scheme, target.host, target.port);

            let state = states.entry(target.id).or_insert(TargetHealthState {
                consecutive_failures: 0,
                consecutive_successes: 0,
                healthy: target.healthy,
            });

            let check_result = client.get(&url).send().await;

            match check_result {
                Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {
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
                }
                _ => {
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
}

/// Update target health status in the database.
async fn update_target_health(pool: &PgPool, target_id: &Uuid, healthy: bool) {
    let result = sqlx::query("UPDATE targets SET healthy = $1, updated_at = NOW() WHERE id = $2")
        .bind(healthy)
        .bind(target_id)
        .execute(pool)
        .await;

    if let Err(e) = result {
        tracing::error!(target_id = %target_id, error = %e, "Failed to update target health in DB");
    }
}
