use chrono::Utc;
use shared::models::{ApiKey, HeaderRule, RateLimit, Route, Target, Upstream};
use uuid::Uuid;

pub fn make_upstream() -> Upstream {
    Upstream {
        id: Uuid::new_v4(),
        name: "test-upstream".to_string(),
        algorithm: "round_robin".to_string(),
        circuit_breaker_threshold: None,
        circuit_breaker_duration_secs: 30,
        active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_target(upstream_id: Uuid) -> Target {
    Target {
        id: Uuid::new_v4(),
        upstream_id,
        host: "127.0.0.1".to_string(),
        port: 8080,
        weight: 1,
        healthy: true,
        tls: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_route(upstream_id: Uuid, path_prefix: &str) -> Route {
    Route {
        id: Uuid::new_v4(),
        name: format!("route-{}", path_prefix),
        path_prefix: path_prefix.to_string(),
        methods: None,
        upstream_id,
        strip_prefix: false,
        upstream_path_prefix: None,
        service_id: None,
        max_body_bytes: None,
        timeout_ms: None,
        retries: 0,
        auth_skip: false,
        active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_api_key(route_id: Option<Uuid>, key_hash: &str) -> ApiKey {
    ApiKey {
        id: Uuid::new_v4(),
        name: "test-key".to_string(),
        key_hash: key_hash.to_string(),
        route_id,
        active: true,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_rate_limit(route_id: Uuid, rps: i32) -> RateLimit {
    RateLimit {
        id: Uuid::new_v4(),
        route_id,
        requests_per_second: rps,
        requests_per_minute: None,
        requests_per_hour: None,
        limit_by: "ip".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_header_rule(route_id: Uuid, action: &str, header_name: &str) -> HeaderRule {
    HeaderRule {
        id: Uuid::new_v4(),
        route_id,
        phase: "request".to_string(),
        action: action.to_string(),
        header_name: header_name.to_string(),
        header_value: if action == "remove" {
            None
        } else {
            Some("test-value".to_string())
        },
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}
