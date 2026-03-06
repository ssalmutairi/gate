use arc_swap::ArcSwap;
use axum::extract::Query;
use axum::Extension;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::proxy_core::router::GatewayConfig;
use crate::request_stats::RequestStats;

pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub async fn stats(
    Extension(request_stats): Extension<Arc<RequestStats>>,
    Extension(gateway_config): Extension<Arc<ArcSwap<GatewayConfig>>>,
) -> Json<Value> {
    let snap = request_stats.snapshot();
    let active_routes = gateway_config.load().routes.len();
    Json(json!({
        "total_requests_today": snap.total_requests,
        "error_rate": snap.error_rate,
        "avg_latency_ms": snap.avg_latency_ms,
        "p95_latency_ms": snap.p95_latency_ms,
        "active_routes": active_routes
    }))
}

#[derive(Deserialize)]
pub struct LogsParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub route_id: Option<String>,
    pub status: Option<i32>,
    pub method: Option<String>,
}

/// Standalone tracks aggregate stats in memory but does not store per-request logs.
pub async fn logs(Query(params): Query<LogsParams>) -> Json<Value> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    Json(json!({
        "data": [],
        "total": 0,
        "page": page,
        "limit": limit
    }))
}
