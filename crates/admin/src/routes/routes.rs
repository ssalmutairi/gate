use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::upstreams::PaginationParams;

// --- DTOs ---

#[derive(Deserialize)]
pub struct CreateRoute {
    pub name: String,
    pub path_prefix: String,
    pub methods: Option<Vec<String>>,
    pub upstream_id: Uuid,
    pub strip_prefix: Option<bool>,
    pub upstream_path_prefix: Option<String>,
    pub max_body_bytes: Option<i64>,
    pub auth_skip: Option<bool>,
    pub timeout_ms: Option<i32>,
    pub retries: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateRoute {
    pub name: Option<String>,
    pub path_prefix: Option<String>,
    pub methods: Option<Vec<String>>,
    pub upstream_id: Option<Uuid>,
    pub strip_prefix: Option<bool>,
    pub upstream_path_prefix: Option<String>,
    pub max_body_bytes: Option<Option<i64>>,
    pub auth_skip: Option<bool>,
    pub active: Option<bool>,
    pub timeout_ms: Option<Option<i32>>,
    pub retries: Option<i32>,
}

#[derive(Serialize)]
pub struct RouteResponse {
    pub id: Uuid,
    pub name: String,
    pub path_prefix: String,
    pub methods: Option<Vec<String>>,
    pub upstream_id: Uuid,
    pub upstream_name: Option<String>,
    pub strip_prefix: bool,
    pub upstream_path_prefix: Option<String>,
    pub service_id: Option<Uuid>,
    pub max_body_bytes: Option<i64>,
    pub timeout_ms: Option<i32>,
    pub retries: i32,
    pub auth_skip: bool,
    pub active: bool,
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

// --- Helpers ---

#[derive(sqlx::FromRow)]
struct RouteWithUpstream {
    id: Uuid,
    name: String,
    path_prefix: String,
    methods: Option<Vec<String>>,
    upstream_id: Uuid,
    upstream_name: Option<String>,
    strip_prefix: bool,
    upstream_path_prefix: Option<String>,
    service_id: Option<Uuid>,
    max_body_bytes: Option<i64>,
    timeout_ms: Option<i32>,
    retries: i32,
    auth_skip: bool,
    active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<RouteWithUpstream> for RouteResponse {
    fn from(r: RouteWithUpstream) -> Self {
        Self {
            id: r.id,
            name: r.name,
            path_prefix: r.path_prefix,
            methods: r.methods,
            upstream_id: r.upstream_id,
            upstream_name: r.upstream_name,
            strip_prefix: r.strip_prefix,
            upstream_path_prefix: r.upstream_path_prefix,
            service_id: r.service_id,
            max_body_bytes: r.max_body_bytes,
            timeout_ms: r.timeout_ms,
            retries: r.retries,
            auth_skip: r.auth_skip,
            active: r.active,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// --- Handlers ---

pub async fn list_routes(
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<RouteResponse>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM routes")
        .fetch_one(&pool)
        .await?;

    let rows: Vec<RouteWithUpstream> = sqlx::query_as(
        r#"SELECT r.id, r.name, r.path_prefix, r.methods, r.upstream_id,
                  u.name as upstream_name, r.strip_prefix,
                  r.upstream_path_prefix, r.service_id, r.max_body_bytes,
                  r.timeout_ms, r.retries,
                  r.auth_skip, r.active, r.created_at, r.updated_at
           FROM routes r
           LEFT JOIN upstreams u ON u.id = r.upstream_id
           ORDER BY r.created_at DESC
           LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(Json(ListResponse {
        data: rows.into_iter().map(RouteResponse::from).collect(),
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_route(
    State(pool): State<PgPool>,
    Json(body): Json<CreateRoute>,
) -> Result<(axum::http::StatusCode, Json<RouteResponse>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if !body.path_prefix.starts_with('/') {
        return Err(AppError::Validation("path_prefix must start with '/'".into()));
    }

    let strip_prefix = body.strip_prefix.unwrap_or(false);
    let auth_skip = body.auth_skip.unwrap_or(false);
    let retries = body.retries.unwrap_or(0);

    // Validate resilience fields
    if retries < 0 || retries > 3 {
        return Err(AppError::Validation("retries must be between 0 and 3".into()));
    }
    if let Some(timeout) = body.timeout_ms {
        if timeout < 100 || timeout > 300_000 {
            return Err(AppError::Validation("timeout_ms must be between 100 and 300000".into()));
        }
    }

    let route: shared::models::Route = sqlx::query_as(
        "INSERT INTO routes (name, path_prefix, methods, upstream_id, strip_prefix, upstream_path_prefix, max_body_bytes, auth_skip, timeout_ms, retries) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
    )
    .bind(body.name.trim())
    .bind(&body.path_prefix)
    .bind(&body.methods)
    .bind(body.upstream_id)
    .bind(strip_prefix)
    .bind(&body.upstream_path_prefix)
    .bind(body.max_body_bytes)
    .bind(auth_skip)
    .bind(body.timeout_ms)
    .bind(retries)
    .fetch_one(&pool)
    .await?;

    // Fetch upstream name
    let upstream_name: Option<(String,)> =
        sqlx::query_as("SELECT name FROM upstreams WHERE id = $1")
            .bind(route.upstream_id)
            .fetch_optional(&pool)
            .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(RouteResponse {
            id: route.id,
            name: route.name,
            path_prefix: route.path_prefix,
            methods: route.methods,
            upstream_id: route.upstream_id,
            upstream_name: upstream_name.map(|u| u.0),
            strip_prefix: route.strip_prefix,
            upstream_path_prefix: route.upstream_path_prefix,
            service_id: route.service_id,
            max_body_bytes: route.max_body_bytes,
            timeout_ms: route.timeout_ms,
            retries: route.retries,
            auth_skip: route.auth_skip,
            active: route.active,
            created_at: route.created_at,
            updated_at: route.updated_at,
        }),
    ))
}

pub async fn get_route(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<RouteResponse>, AppError> {
    let row: RouteWithUpstream = sqlx::query_as(
        r#"SELECT r.id, r.name, r.path_prefix, r.methods, r.upstream_id,
                  u.name as upstream_name, r.strip_prefix,
                  r.upstream_path_prefix, r.service_id, r.max_body_bytes,
                  r.timeout_ms, r.retries,
                  r.auth_skip, r.active, r.created_at, r.updated_at
           FROM routes r
           LEFT JOIN upstreams u ON u.id = r.upstream_id
           WHERE r.id = $1"#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Route not found".into()))?;

    Ok(Json(RouteResponse::from(row)))
}

pub async fn update_route(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRoute>,
) -> Result<Json<RouteResponse>, AppError> {
    let existing: shared::models::Route =
        sqlx::query_as("SELECT * FROM routes WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Route not found".into()))?;

    let name = body.name.unwrap_or(existing.name);
    let path_prefix = body.path_prefix.unwrap_or(existing.path_prefix);
    let methods = if body.methods.is_some() {
        body.methods
    } else {
        existing.methods
    };
    let upstream_id = body.upstream_id.unwrap_or(existing.upstream_id);
    let strip_prefix = body.strip_prefix.unwrap_or(existing.strip_prefix);
    let upstream_path_prefix = if body.upstream_path_prefix.is_some() {
        body.upstream_path_prefix
    } else {
        existing.upstream_path_prefix
    };
    let max_body_bytes = if let Some(mbb) = body.max_body_bytes {
        mbb
    } else {
        existing.max_body_bytes
    };
    let auth_skip = body.auth_skip.unwrap_or(existing.auth_skip);
    let active = body.active.unwrap_or(existing.active);
    let timeout_ms = if let Some(tms) = body.timeout_ms {
        tms
    } else {
        existing.timeout_ms
    };
    let retries = body.retries.unwrap_or(existing.retries);

    if !path_prefix.starts_with('/') {
        return Err(AppError::Validation("path_prefix must start with '/'".into()));
    }

    // Validate resilience fields
    if retries < 0 || retries > 3 {
        return Err(AppError::Validation("retries must be between 0 and 3".into()));
    }
    if let Some(timeout) = timeout_ms {
        if timeout < 100 || timeout > 300_000 {
            return Err(AppError::Validation("timeout_ms must be between 100 and 300000".into()));
        }
    }

    let updated: shared::models::Route = sqlx::query_as(
        "UPDATE routes SET name = $1, path_prefix = $2, methods = $3, upstream_id = $4, strip_prefix = $5, upstream_path_prefix = $6, max_body_bytes = $7, auth_skip = $8, active = $9, timeout_ms = $10, retries = $11, updated_at = now() WHERE id = $12 RETURNING *",
    )
    .bind(&name)
    .bind(&path_prefix)
    .bind(&methods)
    .bind(upstream_id)
    .bind(strip_prefix)
    .bind(&upstream_path_prefix)
    .bind(max_body_bytes)
    .bind(auth_skip)
    .bind(active)
    .bind(timeout_ms)
    .bind(retries)
    .bind(id)
    .fetch_one(&pool)
    .await?;

    let upstream_name: Option<(String,)> =
        sqlx::query_as("SELECT name FROM upstreams WHERE id = $1")
            .bind(updated.upstream_id)
            .fetch_optional(&pool)
            .await?;

    Ok(Json(RouteResponse {
        id: updated.id,
        name: updated.name,
        path_prefix: updated.path_prefix,
        methods: updated.methods,
        upstream_id: updated.upstream_id,
        upstream_name: upstream_name.map(|u| u.0),
        strip_prefix: updated.strip_prefix,
        upstream_path_prefix: updated.upstream_path_prefix,
        service_id: updated.service_id,
        max_body_bytes: updated.max_body_bytes,
        timeout_ms: updated.timeout_ms,
        retries: updated.retries,
        auth_skip: updated.auth_skip,
        active: updated.active,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
    }))
}

pub async fn delete_route(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM routes WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Route not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
