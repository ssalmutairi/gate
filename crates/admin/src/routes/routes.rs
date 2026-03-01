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
}

#[derive(Deserialize)]
pub struct UpdateRoute {
    pub name: Option<String>,
    pub path_prefix: Option<String>,
    pub methods: Option<Vec<String>>,
    pub upstream_id: Option<Uuid>,
    pub strip_prefix: Option<bool>,
    pub active: Option<bool>,
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
                  u.name as upstream_name, r.strip_prefix, r.active,
                  r.created_at, r.updated_at
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

    let route: shared::models::Route = sqlx::query_as(
        "INSERT INTO routes (name, path_prefix, methods, upstream_id, strip_prefix) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(body.name.trim())
    .bind(&body.path_prefix)
    .bind(&body.methods)
    .bind(body.upstream_id)
    .bind(strip_prefix)
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
                  u.name as upstream_name, r.strip_prefix, r.active,
                  r.created_at, r.updated_at
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
    let active = body.active.unwrap_or(existing.active);

    if !path_prefix.starts_with('/') {
        return Err(AppError::Validation("path_prefix must start with '/'".into()));
    }

    let updated: shared::models::Route = sqlx::query_as(
        "UPDATE routes SET name = $1, path_prefix = $2, methods = $3, upstream_id = $4, strip_prefix = $5, active = $6, updated_at = now() WHERE id = $7 RETURNING *",
    )
    .bind(&name)
    .bind(&path_prefix)
    .bind(&methods)
    .bind(upstream_id)
    .bind(strip_prefix)
    .bind(active)
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
