use crate::models::*;
use crate::proxy_core::router::GatewayConfig;
use crate::proxy_core::soap::SoapServiceMeta;
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;

pub fn collect_cb_configs(config: &GatewayConfig) -> Vec<(Uuid, u32, u32)> {
    let mut cb_configs = Vec::new();
    for (upstream_id, upstream) in &config.upstreams {
        if let Some(threshold) = upstream.circuit_breaker_threshold {
            if let Some(targets) = config.targets.get(upstream_id) {
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
    cb_configs
}

pub async fn load_config(pool: &SqlitePool) -> GatewayConfig {
    // Run all independent queries concurrently
    let (
        sqlite_routes,
        sqlite_upstreams,
        sqlite_targets,
        sqlite_api_keys,
        sqlite_rate_limits,
        sqlite_header_rules,
        sqlite_ip_rules,
        soap_rows,
    ) = tokio::join!(
        async {
            sqlx::query_as::<_, SqliteRoute>("SELECT * FROM routes WHERE active = 1")
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, SqliteUpstream>("SELECT * FROM upstreams WHERE active = 1")
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, SqliteTarget>(
                "SELECT t.* FROM targets t INNER JOIN upstreams u ON t.upstream_id = u.id WHERE u.active = 1"
            )
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, SqliteApiKey>("SELECT * FROM api_keys")
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, SqliteRateLimit>("SELECT * FROM rate_limits")
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, SqliteHeaderRule>("SELECT * FROM header_rules")
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, SqliteIpRule>("SELECT * FROM ip_rules")
                .fetch_all(pool)
                .await
                .unwrap_or_default()
        },
        async {
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT id, soap_metadata FROM services WHERE service_type = 'soap' AND soap_metadata IS NOT NULL",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default()
        },
    );

    let routes: Vec<shared::models::Route> = sqlite_routes.into_iter().map(Into::into).collect();
    let upstreams: Vec<shared::models::Upstream> = sqlite_upstreams.into_iter().map(Into::into).collect();
    let targets: Vec<shared::models::Target> = sqlite_targets.into_iter().map(Into::into).collect();
    let api_keys: Vec<shared::models::ApiKey> = sqlite_api_keys.into_iter().map(Into::into).collect();
    let rate_limits: Vec<shared::models::RateLimit> = sqlite_rate_limits.into_iter().map(Into::into).collect();
    let header_rules: Vec<shared::models::HeaderRule> = sqlite_header_rules.into_iter().map(Into::into).collect();
    let ip_rules: Vec<shared::models::IpRule> = sqlite_ip_rules.into_iter().map(Into::into).collect();

    let mut soap_services: HashMap<Uuid, SoapServiceMeta> = HashMap::new();
    for (service_id_str, meta_opt) in soap_rows {
        if let (Ok(service_id), Some(meta_str)) = (Uuid::parse_str(&service_id_str), meta_opt) {
            if let Ok(meta_val) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                if let Some(parsed) = SoapServiceMeta::from_json(&meta_val) {
                    soap_services.insert(service_id, parsed);
                }
            }
        }
    }

    tracing::info!(
        routes = routes.len(),
        upstreams = upstreams.len(),
        targets = targets.len(),
        api_keys = api_keys.len(),
        rate_limits = rate_limits.len(),
        header_rules = header_rules.len(),
        ip_rules = ip_rules.len(),
        soap_services = soap_services.len(),
        "Config loaded from SQLite"
    );

    GatewayConfig::with_soap(
        routes,
        upstreams,
        targets,
        api_keys,
        rate_limits,
        header_rules,
        ip_rules,
        soap_services,
    )
}
