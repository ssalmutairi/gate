use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Upstream {
    pub id: Uuid,
    pub name: String,
    pub algorithm: String,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_duration_secs: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Target {
    pub id: Uuid,
    pub upstream_id: Uuid,
    pub host: String,
    pub port: i32,
    pub weight: i32,
    pub healthy: bool,
    pub tls: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Route {
    pub id: Uuid,
    pub name: String,
    pub path_prefix: String,
    pub methods: Option<Vec<String>>,
    pub upstream_id: Uuid,
    pub strip_prefix: bool,
    pub upstream_path_prefix: Option<String>,
    pub service_id: Option<Uuid>,
    pub max_body_bytes: Option<i64>,
    pub timeout_ms: Option<i32>,
    pub retries: i32,
    pub auth_skip: bool,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub route_id: Option<Uuid>,
    pub active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RateLimit {
    pub id: Uuid,
    pub route_id: Uuid,
    pub requests_per_second: i32,
    pub requests_per_minute: Option<i32>,
    pub requests_per_hour: Option<i32>,
    pub limit_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RequestLog {
    pub id: Uuid,
    pub route_id: Option<Uuid>,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub latency_ms: f64,
    pub client_ip: String,
    pub upstream_target: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Service {
    pub id: Uuid,
    pub namespace: String,
    pub version: i32,
    pub spec_url: String,
    pub spec_hash: String,
    pub upstream_id: Uuid,
    pub route_id: Option<Uuid>,
    pub description: String,
    pub tags: Vec<String>,
    pub status: String,
    pub spec_content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HeaderRule {
    pub id: Uuid,
    pub route_id: Uuid,
    pub phase: String,
    pub action: String,
    pub header_name: String,
    pub header_value: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
