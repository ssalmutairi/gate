use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

// --- DTOs ---

#[derive(Deserialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateUpstream {
    pub name: String,
    pub algorithm: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateUpstream {
    pub name: Option<String>,
    pub algorithm: Option<String>,
    pub active: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateTarget {
    pub host: String,
    pub port: i32,
    pub weight: Option<i32>,
}

#[derive(Serialize)]
pub struct UpstreamResponse {
    pub id: Uuid,
    pub name: String,
    pub algorithm: String,
    pub active: bool,
    pub targets: Vec<TargetResponse>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct TargetResponse {
    pub id: Uuid,
    pub upstream_id: Uuid,
    pub host: String,
    pub port: i32,
    pub weight: i32,
    pub healthy: bool,
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

pub async fn list_upstreams(
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<UpstreamResponse>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM upstreams")
        .fetch_one(&pool)
        .await?;

    let upstreams: Vec<shared::models::Upstream> =
        sqlx::query_as("SELECT * FROM upstreams ORDER BY created_at DESC LIMIT $1 OFFSET $2")
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?;

    let mut results = Vec::with_capacity(upstreams.len());
    for u in upstreams {
        let targets: Vec<TargetResponse> = sqlx::query_as(
            "SELECT * FROM targets WHERE upstream_id = $1 ORDER BY created_at",
        )
        .bind(u.id)
        .fetch_all(&pool)
        .await?;

        results.push(UpstreamResponse {
            id: u.id,
            name: u.name,
            algorithm: u.algorithm,
            active: u.active,
            targets,
            created_at: u.created_at,
            updated_at: u.updated_at,
        });
    }

    Ok(Json(ListResponse {
        data: results,
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_upstream(
    State(pool): State<PgPool>,
    Json(body): Json<CreateUpstream>,
) -> Result<(axum::http::StatusCode, Json<UpstreamResponse>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }

    let algorithm = body.algorithm.unwrap_or_else(|| "round_robin".into());
    if algorithm != "round_robin" && algorithm != "weighted_round_robin" && algorithm != "least_connections" {
        return Err(AppError::Validation(
            "algorithm must be 'round_robin', 'weighted_round_robin', or 'least_connections'".into(),
        ));
    }

    let upstream: shared::models::Upstream = sqlx::query_as(
        "INSERT INTO upstreams (name, algorithm) VALUES ($1, $2) RETURNING *",
    )
    .bind(body.name.trim())
    .bind(&algorithm)
    .fetch_one(&pool)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(UpstreamResponse {
            id: upstream.id,
            name: upstream.name,
            algorithm: upstream.algorithm,
            active: upstream.active,
            targets: vec![],
            created_at: upstream.created_at,
            updated_at: upstream.updated_at,
        }),
    ))
}

pub async fn get_upstream(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<UpstreamResponse>, AppError> {
    let upstream: shared::models::Upstream =
        sqlx::query_as("SELECT * FROM upstreams WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Upstream not found".into()))?;

    let targets: Vec<TargetResponse> =
        sqlx::query_as("SELECT * FROM targets WHERE upstream_id = $1 ORDER BY created_at")
            .bind(id)
            .fetch_all(&pool)
            .await?;

    Ok(Json(UpstreamResponse {
        id: upstream.id,
        name: upstream.name,
        algorithm: upstream.algorithm,
        active: upstream.active,
        targets,
        created_at: upstream.created_at,
        updated_at: upstream.updated_at,
    }))
}

pub async fn update_upstream(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUpstream>,
) -> Result<Json<UpstreamResponse>, AppError> {
    // Check exists
    let existing: shared::models::Upstream =
        sqlx::query_as("SELECT * FROM upstreams WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Upstream not found".into()))?;

    let name = body
        .name
        .unwrap_or(existing.name);
    let algorithm = body.algorithm.unwrap_or(existing.algorithm);
    let active = body.active.unwrap_or(existing.active);

    if algorithm != "round_robin" && algorithm != "weighted_round_robin" && algorithm != "least_connections" {
        return Err(AppError::Validation(
            "algorithm must be 'round_robin', 'weighted_round_robin', or 'least_connections'".into(),
        ));
    }

    let updated: shared::models::Upstream = sqlx::query_as(
        "UPDATE upstreams SET name = $1, algorithm = $2, active = $3, updated_at = now() WHERE id = $4 RETURNING *",
    )
    .bind(&name)
    .bind(&algorithm)
    .bind(active)
    .bind(id)
    .fetch_one(&pool)
    .await?;

    let targets: Vec<TargetResponse> =
        sqlx::query_as("SELECT * FROM targets WHERE upstream_id = $1 ORDER BY created_at")
            .bind(id)
            .fetch_all(&pool)
            .await?;

    Ok(Json(UpstreamResponse {
        id: updated.id,
        name: updated.name,
        algorithm: updated.algorithm,
        active: updated.active,
        targets,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
    }))
}

pub async fn delete_upstream(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM upstreams WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Upstream not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Target handlers ---

pub async fn add_target(
    State(pool): State<PgPool>,
    Path(upstream_id): Path<Uuid>,
    Json(body): Json<CreateTarget>,
) -> Result<(axum::http::StatusCode, Json<TargetResponse>), AppError> {
    // Verify upstream exists
    sqlx::query("SELECT id FROM upstreams WHERE id = $1")
        .bind(upstream_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Upstream not found".into()))?;

    if body.host.trim().is_empty() {
        return Err(AppError::Validation("host is required".into()));
    }
    if body.port <= 0 || body.port > 65535 {
        return Err(AppError::Validation("port must be between 1 and 65535".into()));
    }

    let weight = body.weight.unwrap_or(1);

    let target: TargetResponse = sqlx::query_as(
        "INSERT INTO targets (upstream_id, host, port, weight) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(upstream_id)
    .bind(body.host.trim())
    .bind(body.port)
    .bind(weight)
    .fetch_one(&pool)
    .await?;

    // Touch upstream updated_at to trigger hot reload
    sqlx::query("UPDATE upstreams SET updated_at = now() WHERE id = $1")
        .bind(upstream_id)
        .execute(&pool)
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(target)))
}

pub async fn delete_target(
    State(pool): State<PgPool>,
    Path((upstream_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, AppError> {
    let result =
        sqlx::query("DELETE FROM targets WHERE id = $1 AND upstream_id = $2")
            .bind(target_id)
            .bind(upstream_id)
            .execute(&pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Target not found".into()));
    }

    // Touch upstream updated_at to trigger hot reload
    sqlx::query("UPDATE upstreams SET updated_at = now() WHERE id = $1")
        .bind(upstream_id)
        .execute(&pool)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
