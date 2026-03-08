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

/// Trigger an immediate config reload from the database.
pub async fn reload_config(
    axum::extract::State(pool): axum::extract::State<sqlx::SqlitePool>,
    Extension(gateway_config): Extension<Arc<ArcSwap<GatewayConfig>>>,
) -> Json<Value> {
    let new_config = crate::config_loader::load_config(&pool).await;
    gateway_config.store(Arc::new(new_config));
    tracing::info!("Config reloaded via admin API");
    Json(json!({ "status": "reloaded" }))
}

/// Returns the latest request logs from the in-memory ring buffer.
pub async fn logs(
    Extension(request_stats): Extension<Arc<RequestStats>>,
    Query(params): Query<LogsParams>,
) -> Json<Value> {
    let page = params.page.unwrap_or(1).max(1) as usize;
    let limit = params.limit.unwrap_or(20).clamp(1, 100) as usize;
    let (mut entries, total) = request_stats.get_logs(page, limit);

    // Apply optional filters
    if let Some(ref method) = params.method {
        entries.retain(|e| e.method == *method);
    }
    if let Some(status) = params.status {
        entries.retain(|e| e.status_code == status);
    }
    if let Some(ref route_id) = params.route_id {
        entries.retain(|e| e.route_id.as_deref() == Some(route_id.as_str()));
    }

    Json(json!({
        "data": entries,
        "total": total,
        "page": page,
        "limit": limit
    }))
}
