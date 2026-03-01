use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::errors::AppError;

#[derive(Serialize)]
pub struct StatsResponse {
    pub total_requests_today: i64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub active_routes: i64,
}

pub async fn get_stats(State(pool): State<PgPool>) -> Result<Json<StatsResponse>, AppError> {
    let row: (i64, f64, f64, f64) = sqlx::query_as(
        r#"SELECT
            COUNT(*) as total,
            COALESCE(AVG(CASE WHEN status_code >= 400 THEN 1.0 ELSE 0.0 END), 0)::float8 as error_rate,
            COALESCE(AVG(latency_ms), 0)::float8 as avg_latency,
            COALESCE(PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY latency_ms), 0)::float8 as p95_latency
        FROM request_logs
        WHERE created_at >= CURRENT_DATE"#,
    )
    .fetch_one(&pool)
    .await?;

    let active_routes: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM routes WHERE active = true")
        .fetch_one(&pool)
        .await?;

    Ok(Json(StatsResponse {
        total_requests_today: row.0,
        error_rate: row.1,
        avg_latency_ms: row.2,
        p95_latency_ms: row.3,
        active_routes: active_routes.0,
    }))
}

// --- Request Logs ---

#[derive(Deserialize)]
pub struct LogsQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub route_id: Option<String>,
    pub status: Option<i32>,
    pub method: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct LogEntry {
    pub id: uuid::Uuid,
    pub route_id: Option<uuid::Uuid>,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub latency_ms: f64,
    pub client_ip: String,
    pub upstream_target: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub data: Vec<LogEntry>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

pub async fn get_logs(
    State(pool): State<PgPool>,
    Query(params): Query<LogsQuery>,
) -> Result<Json<LogsResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = (page - 1) * limit;

    // Build dynamic query based on filters
    let mut conditions = Vec::new();
    let mut idx = 1;

    if params.route_id.is_some() {
        conditions.push(format!("route_id = ${}", idx));
        idx += 1;
    }
    if params.status.is_some() {
        conditions.push(format!("status_code = ${}", idx));
        idx += 1;
    }
    if params.method.is_some() {
        conditions.push(format!("method = ${}", idx));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM request_logs {}", where_clause);
    let data_query = format!(
        "SELECT * FROM request_logs {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
        where_clause, idx, idx + 1
    );

    // Build and execute count query
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_query);
    if let Some(ref route_id) = params.route_id {
        count_q = count_q.bind(uuid::Uuid::parse_str(route_id).unwrap_or_default());
    }
    if let Some(status) = params.status {
        count_q = count_q.bind(status);
    }
    if let Some(ref method) = params.method {
        count_q = count_q.bind(method);
    }
    let total = count_q.fetch_one(&pool).await?.0;

    // Build and execute data query
    let mut data_q = sqlx::query_as::<_, LogEntry>(&data_query);
    if let Some(ref route_id) = params.route_id {
        data_q = data_q.bind(uuid::Uuid::parse_str(route_id).unwrap_or_default());
    }
    if let Some(status) = params.status {
        data_q = data_q.bind(status);
    }
    if let Some(ref method) = params.method {
        data_q = data_q.bind(method);
    }
    data_q = data_q.bind(limit).bind(offset);

    let data = data_q.fetch_all(&pool).await?;

    Ok(Json(LogsResponse {
        data,
        total,
        page,
        limit,
    }))
}
