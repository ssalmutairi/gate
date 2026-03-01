use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::upstreams::PaginationParams;

// --- DTOs ---

#[derive(Deserialize)]
pub struct CreateRateLimit {
    pub route_id: Uuid,
    pub requests_per_second: i32,
    pub requests_per_minute: Option<i32>,
    pub requests_per_hour: Option<i32>,
    pub limit_by: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRateLimit {
    pub requests_per_second: Option<i32>,
    pub requests_per_minute: Option<i32>,
    pub requests_per_hour: Option<i32>,
    pub limit_by: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RateLimitResponse {
    pub id: Uuid,
    pub route_id: Uuid,
    pub requests_per_second: i32,
    pub requests_per_minute: Option<i32>,
    pub requests_per_hour: Option<i32>,
    pub limit_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

// --- Handlers ---

pub async fn list_rate_limits(
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<RateLimitResponse>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM rate_limits")
        .fetch_one(&pool)
        .await?;

    let data: Vec<RateLimitResponse> = sqlx::query_as(
        "SELECT * FROM rate_limits ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(Json(ListResponse {
        data,
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_rate_limit(
    State(pool): State<PgPool>,
    Json(body): Json<CreateRateLimit>,
) -> Result<(axum::http::StatusCode, Json<RateLimitResponse>), AppError> {
    if body.requests_per_second <= 0 {
        return Err(AppError::Validation(
            "requests_per_second must be positive".into(),
        ));
    }

    let limit_by = body.limit_by.unwrap_or_else(|| "ip".into());
    if limit_by != "ip" && limit_by != "api_key" {
        return Err(AppError::Validation(
            "limit_by must be 'ip' or 'api_key'".into(),
        ));
    }

    // Verify route exists
    sqlx::query("SELECT id FROM routes WHERE id = $1")
        .bind(body.route_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::Validation("route_id does not exist".into()))?;

    let rl: RateLimitResponse = sqlx::query_as(
        "INSERT INTO rate_limits (route_id, requests_per_second, requests_per_minute, requests_per_hour, limit_by) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(body.route_id)
    .bind(body.requests_per_second)
    .bind(body.requests_per_minute)
    .bind(body.requests_per_hour)
    .bind(&limit_by)
    .fetch_one(&pool)
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(rl)))
}

pub async fn update_rate_limit(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRateLimit>,
) -> Result<Json<RateLimitResponse>, AppError> {
    let existing: shared::models::RateLimit =
        sqlx::query_as("SELECT * FROM rate_limits WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Rate limit not found".into()))?;

    let rps = body.requests_per_second.unwrap_or(existing.requests_per_second);
    let rpm = if body.requests_per_minute.is_some() {
        body.requests_per_minute
    } else {
        existing.requests_per_minute
    };
    let rph = if body.requests_per_hour.is_some() {
        body.requests_per_hour
    } else {
        existing.requests_per_hour
    };
    let limit_by = body.limit_by.unwrap_or(existing.limit_by);

    if limit_by != "ip" && limit_by != "api_key" {
        return Err(AppError::Validation(
            "limit_by must be 'ip' or 'api_key'".into(),
        ));
    }

    let updated: RateLimitResponse = sqlx::query_as(
        "UPDATE rate_limits SET requests_per_second = $1, requests_per_minute = $2, requests_per_hour = $3, limit_by = $4, updated_at = now() WHERE id = $5 RETURNING *",
    )
    .bind(rps)
    .bind(rpm)
    .bind(rph)
    .bind(&limit_by)
    .bind(id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(updated))
}

pub async fn delete_rate_limit(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM rate_limits WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Rate limit not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
