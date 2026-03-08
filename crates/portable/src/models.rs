use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// SQLite-compatible row types that use String for arrays/JSON fields.
// These convert to shared::models types via From impls.

pub fn parse_uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap_or_else(|e| {
        tracing::warn!(value = s, error = %e, "Failed to parse UUID, using nil");
        Uuid::nil()
    })
}

pub fn parse_dt(s: &str) -> DateTime<Utc> {
    // Try ISO 8601 with timezone, then without
    s.parse::<DateTime<Utc>>()
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .unwrap_or_else(|e| {
            tracing::warn!(value = s, error = %e, "Failed to parse datetime, using epoch");
            DateTime::default()
        })
}

// --- Route (methods is JSON text in SQLite) ---

#[derive(Debug, Clone, FromRow)]
pub struct SqliteRoute {
    pub id: String,
    pub name: String,
    pub path_prefix: String,
    pub methods: Option<String>,
    pub upstream_id: String,
    pub strip_prefix: bool,
    pub upstream_path_prefix: Option<String>,
    pub service_id: Option<String>,
    pub max_body_bytes: Option<i64>,
    pub timeout_ms: Option<i32>,
    pub retries: i32,
    pub host_pattern: Option<String>,
    pub cache_ttl_secs: Option<i32>,
    pub auth_skip: bool,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteRoute> for shared::models::Route {
    fn from(r: SqliteRoute) -> Self {
        let methods: Option<Vec<String>> = r
            .methods
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        Self {
            id: parse_uuid(&r.id),
            name: r.name,
            path_prefix: r.path_prefix,
            methods,
            upstream_id: parse_uuid(&r.upstream_id),
            strip_prefix: r.strip_prefix,
            upstream_path_prefix: r.upstream_path_prefix,
            service_id: r.service_id.as_deref().map(parse_uuid),
            max_body_bytes: r.max_body_bytes,
            timeout_ms: r.timeout_ms,
            retries: r.retries,
            host_pattern: r.host_pattern,
            cache_ttl_secs: r.cache_ttl_secs,
            auth_skip: r.auth_skip,
            active: r.active,
            created_at: parse_dt(&r.created_at),
            updated_at: parse_dt(&r.updated_at),
        }
    }
}

// --- Service (tags is JSON text, soap_metadata is JSON text) ---

#[derive(Debug, Clone, FromRow)]
pub struct SqliteService {
    pub id: String,
    pub namespace: String,
    pub version: i32,
    pub spec_url: String,
    pub spec_hash: String,
    pub upstream_id: String,
    pub route_id: Option<String>,
    pub description: String,
    pub tags: String,
    pub status: String,
    pub spec_content: Option<String>,
    pub service_type: String,
    pub soap_metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteService> for shared::models::Service {
    fn from(s: SqliteService) -> Self {
        let tags: Vec<String> = serde_json::from_str(&s.tags).unwrap_or_default();
        let soap_metadata: Option<serde_json::Value> = s
            .soap_metadata
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        Self {
            id: parse_uuid(&s.id),
            namespace: s.namespace,
            version: s.version,
            spec_url: s.spec_url,
            spec_hash: s.spec_hash,
            upstream_id: parse_uuid(&s.upstream_id),
            route_id: s.route_id.as_deref().map(parse_uuid),
            description: s.description,
            tags,
            status: s.status,
            spec_content: s.spec_content,
            service_type: s.service_type,
            soap_metadata,
            created_at: parse_dt(&s.created_at),
            updated_at: parse_dt(&s.updated_at),
        }
    }
}

// --- Simple row types for SQLite (no array/JSON fields) ---

#[derive(Debug, Clone, FromRow)]
pub struct SqliteUpstream {
    pub id: String,
    pub name: String,
    pub algorithm: String,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_duration_secs: i32,
    pub active: bool,
    pub tls_ca_cert: Option<String>,
    pub tls_client_cert: Option<String>,
    pub tls_client_key: Option<String>,
    pub tls_skip_verify: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteUpstream> for shared::models::Upstream {
    fn from(u: SqliteUpstream) -> Self {
        Self {
            id: parse_uuid(&u.id),
            name: u.name,
            algorithm: u.algorithm,
            circuit_breaker_threshold: u.circuit_breaker_threshold,
            circuit_breaker_duration_secs: u.circuit_breaker_duration_secs,
            active: u.active,
            tls_ca_cert: u.tls_ca_cert,
            tls_client_cert: u.tls_client_cert,
            tls_client_key: u.tls_client_key,
            tls_skip_verify: u.tls_skip_verify,
            created_at: parse_dt(&u.created_at),
            updated_at: parse_dt(&u.updated_at),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SqliteTarget {
    pub id: String,
    pub upstream_id: String,
    pub host: String,
    pub port: i32,
    pub weight: i32,
    pub healthy: bool,
    pub tls: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteTarget> for shared::models::Target {
    fn from(t: SqliteTarget) -> Self {
        Self {
            id: parse_uuid(&t.id),
            upstream_id: parse_uuid(&t.upstream_id),
            host: t.host,
            port: t.port,
            weight: t.weight,
            healthy: t.healthy,
            tls: t.tls,
            created_at: parse_dt(&t.created_at),
            updated_at: parse_dt(&t.updated_at),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct SqliteApiKey {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub route_id: Option<String>,
    pub active: bool,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteApiKey> for shared::models::ApiKey {
    fn from(k: SqliteApiKey) -> Self {
        Self {
            id: parse_uuid(&k.id),
            name: k.name,
            key_hash: k.key_hash,
            route_id: k.route_id.as_deref().map(parse_uuid),
            active: k.active,
            expires_at: k.expires_at.as_deref().map(parse_dt),
            created_at: parse_dt(&k.created_at),
            updated_at: parse_dt(&k.updated_at),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SqliteRateLimit {
    pub id: String,
    pub route_id: String,
    pub requests_per_second: i32,
    pub requests_per_minute: Option<i32>,
    pub requests_per_hour: Option<i32>,
    pub limit_by: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteRateLimit> for shared::models::RateLimit {
    fn from(r: SqliteRateLimit) -> Self {
        Self {
            id: parse_uuid(&r.id),
            route_id: parse_uuid(&r.route_id),
            requests_per_second: r.requests_per_second,
            requests_per_minute: r.requests_per_minute,
            requests_per_hour: r.requests_per_hour,
            limit_by: r.limit_by,
            created_at: parse_dt(&r.created_at),
            updated_at: parse_dt(&r.updated_at),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct SqliteHeaderRule {
    pub id: String,
    pub route_id: String,
    pub phase: String,
    pub action: String,
    pub header_name: String,
    pub header_value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteHeaderRule> for shared::models::HeaderRule {
    fn from(h: SqliteHeaderRule) -> Self {
        Self {
            id: parse_uuid(&h.id),
            route_id: parse_uuid(&h.route_id),
            phase: h.phase,
            action: h.action,
            header_name: h.header_name,
            header_value: h.header_value,
            created_at: parse_dt(&h.created_at),
            updated_at: parse_dt(&h.updated_at),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct SqliteIpRule {
    pub id: String,
    pub route_id: String,
    pub cidr: String,
    pub action: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SqliteIpRule> for shared::models::IpRule {
    fn from(r: SqliteIpRule) -> Self {
        Self {
            id: parse_uuid(&r.id),
            route_id: parse_uuid(&r.route_id),
            cidr: r.cidr,
            action: r.action,
            description: r.description,
            created_at: parse_dt(&r.created_at),
            updated_at: parse_dt(&r.updated_at),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uuid_valid() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_uuid(id);
        assert_eq!(result.to_string(), id);
    }

    #[test]
    fn parse_uuid_invalid_returns_nil() {
        let result = parse_uuid("not-a-uuid");
        assert_eq!(result, Uuid::nil());
    }

    #[test]
    fn parse_dt_iso8601_with_tz() {
        let dt = parse_dt("2024-01-15T10:30:00+00:00");
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
    }

    #[test]
    fn parse_dt_sqlite_format() {
        let dt = parse_dt("2024-06-15 14:30:00");
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 6);
    }

    #[test]
    fn parse_dt_invalid_returns_epoch() {
        let dt = parse_dt("not-a-date");
        assert_eq!(dt, DateTime::<Utc>::default());
    }

    #[test]
    fn sqlite_route_to_shared_methods_json() {
        let route = SqliteRoute {
            id: Uuid::new_v4().to_string(),
            name: "test".into(),
            path_prefix: "/api".into(),
            methods: Some(r#"["GET","POST"]"#.into()),
            upstream_id: Uuid::new_v4().to_string(),
            strip_prefix: false,
            upstream_path_prefix: None,
            service_id: None,
            max_body_bytes: None,
            timeout_ms: None,
            retries: 0,
            host_pattern: None,
            cache_ttl_secs: None,
            auth_skip: false,
            active: true,
            created_at: "2024-01-01 00:00:00".into(),
            updated_at: "2024-01-01 00:00:00".into(),
        };
        let shared_route: shared::models::Route = route.into();
        assert_eq!(shared_route.methods, Some(vec!["GET".to_string(), "POST".to_string()]));
    }

    #[test]
    fn sqlite_route_null_methods() {
        let route = SqliteRoute {
            id: Uuid::new_v4().to_string(),
            name: "test".into(),
            path_prefix: "/api".into(),
            methods: None,
            upstream_id: Uuid::new_v4().to_string(),
            strip_prefix: false,
            upstream_path_prefix: None,
            service_id: None,
            max_body_bytes: None,
            timeout_ms: None,
            retries: 0,
            host_pattern: None,
            cache_ttl_secs: None,
            auth_skip: false,
            active: true,
            created_at: "2024-01-01 00:00:00".into(),
            updated_at: "2024-01-01 00:00:00".into(),
        };
        let shared_route: shared::models::Route = route.into();
        assert_eq!(shared_route.methods, None);
    }

    #[test]
    fn sqlite_service_tags_json() {
        let svc = SqliteService {
            id: Uuid::new_v4().to_string(),
            namespace: "test".into(),
            version: 1,
            spec_url: "http://example.com".into(),
            spec_hash: "abc".into(),
            upstream_id: Uuid::new_v4().to_string(),
            route_id: None,
            description: "desc".into(),
            tags: r#"["api","v2"]"#.into(),
            status: "stable".into(),
            spec_content: None,
            service_type: "rest".into(),
            soap_metadata: None,
            created_at: "2024-01-01 00:00:00".into(),
            updated_at: "2024-01-01 00:00:00".into(),
        };
        let shared_svc: shared::models::Service = svc.into();
        assert_eq!(shared_svc.tags, vec!["api", "v2"]);
    }

    use chrono::Datelike;
}

