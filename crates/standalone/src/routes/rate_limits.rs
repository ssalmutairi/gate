use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::SqliteRateLimit;
use super::{ListResponse, PaginationParams};

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

#[derive(Serialize)]
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

impl From<SqliteRateLimit> for RateLimitResponse {
    fn from(r: SqliteRateLimit) -> Self {
        Self {
            id: crate::models::parse_uuid(&r.id),
            route_id: crate::models::parse_uuid(&r.route_id),
            requests_per_second: r.requests_per_second,
            requests_per_minute: r.requests_per_minute,
            requests_per_hour: r.requests_per_hour,
            limit_by: r.limit_by,
            created_at: crate::models::parse_dt(&r.created_at),
            updated_at: crate::models::parse_dt(&r.updated_at),
        }
    }
}

fn validate_limit_by(limit_by: &str) -> Result<(), AppError> {
    if !matches!(limit_by, "ip" | "api_key") {
        return Err(AppError::Validation(
            "limit_by must be 'ip' or 'api_key'".into(),
        ));
    }
    Ok(())
}

pub async fn list_rate_limits(
    State(pool): State<SqlitePool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<RateLimitResponse>>, AppError> {
    let (page, limit, offset) = params.resolve();

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM rate_limits")
        .fetch_one(&pool)
        .await?;

    let data: Vec<SqliteRateLimit> = sqlx::query_as(
        "SELECT * FROM rate_limits ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(Json(ListResponse {
        data: data.into_iter().map(RateLimitResponse::from).collect(),
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_rate_limit(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateRateLimit>,
) -> Result<(axum::http::StatusCode, Json<RateLimitResponse>), AppError> {
    if body.requests_per_second <= 0 {
        return Err(AppError::Validation(
            "requests_per_second must be positive".into(),
        ));
    }

    let limit_by = body.limit_by.unwrap_or_else(|| "ip".into());
    validate_limit_by(&limit_by)?;

    super::ensure_route_exists(&pool, body.route_id).await
        .map_err(|_| AppError::Validation("route_id does not exist".into()))?;

    let id = Uuid::new_v4().to_string();
    let rl: SqliteRateLimit = sqlx::query_as(
        "INSERT INTO rate_limits (id, route_id, requests_per_second, requests_per_minute, requests_per_hour, limit_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING *",
    )
    .bind(&id)
    .bind(body.route_id.to_string())
    .bind(body.requests_per_second)
    .bind(body.requests_per_minute)
    .bind(body.requests_per_hour)
    .bind(&limit_by)
    .fetch_one(&pool)
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(RateLimitResponse::from(rl))))
}

pub async fn update_rate_limit(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRateLimit>,
) -> Result<Json<RateLimitResponse>, AppError> {
    let existing_row: SqliteRateLimit =
        sqlx::query_as("SELECT * FROM rate_limits WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Rate limit not found".into()))?;
    let existing = RateLimitResponse::from(existing_row);

    let rps = body.requests_per_second.unwrap_or(existing.requests_per_second);
    if rps <= 0 {
        return Err(AppError::Validation(
            "requests_per_second must be positive".into(),
        ));
    }
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

    validate_limit_by(&limit_by)?;

    let updated: SqliteRateLimit = sqlx::query_as(
        "UPDATE rate_limits SET requests_per_second = ?1, requests_per_minute = ?2, requests_per_hour = ?3, limit_by = ?4, updated_at = datetime('now') WHERE id = ?5 RETURNING *",
    )
    .bind(rps)
    .bind(rpm)
    .bind(rph)
    .bind(&limit_by)
    .bind(id.to_string())
    .fetch_one(&pool)
    .await?;

    Ok(Json(RateLimitResponse::from(updated)))
}

pub async fn delete_rate_limit(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    super::delete_by_id(&pool, "rate_limits", id, "Rate limit").await
}
