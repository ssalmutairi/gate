use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::upstreams::PaginationParams;

// --- DTOs ---

#[derive(Deserialize)]
pub struct CreateApiKey {
    pub name: String,
    pub route_id: Option<Uuid>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct UpdateApiKey {
    pub name: Option<String>,
    pub active: Option<bool>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub route_id: Option<Uuid>,
    pub active: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct ApiKeyCreatedResponse {
    pub id: Uuid,
    pub name: String,
    pub key: String,
    pub route_id: Option<Uuid>,
    pub active: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub message: String,
}

#[derive(Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

// --- Handlers ---

pub async fn list_api_keys(
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<ApiKeyResponse>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_keys")
        .fetch_one(&pool)
        .await?;

    let keys: Vec<shared::models::ApiKey> = sqlx::query_as(
        "SELECT * FROM api_keys ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let data = keys
        .into_iter()
        .map(|k| ApiKeyResponse {
            id: k.id,
            name: k.name,
            route_id: k.route_id,
            active: k.active,
            expires_at: k.expires_at,
            created_at: k.created_at,
            updated_at: k.updated_at,
        })
        .collect();

    Ok(Json(ListResponse {
        data,
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_api_key(
    State(pool): State<PgPool>,
    Json(body): Json<CreateApiKey>,
) -> Result<(axum::http::StatusCode, Json<ApiKeyCreatedResponse>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }

    // Verify route exists if scoped
    if let Some(route_id) = body.route_id {
        sqlx::query("SELECT id FROM routes WHERE id = $1")
            .bind(route_id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::Validation("route_id does not exist".into()))?;
    }

    // Generate random 32-byte key, prefix with "gw_"
    let random_bytes: [u8; 32] = rand::random();
    let plaintext_key = format!("gw_{}", hex::encode(random_bytes));

    // SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(plaintext_key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let key: shared::models::ApiKey = sqlx::query_as(
        "INSERT INTO api_keys (name, key_hash, route_id, expires_at) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(body.name.trim())
    .bind(&key_hash)
    .bind(body.route_id)
    .bind(body.expires_at)
    .fetch_one(&pool)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ApiKeyCreatedResponse {
            id: key.id,
            name: key.name,
            key: plaintext_key,
            route_id: key.route_id,
            active: key.active,
            expires_at: key.expires_at,
            message: "Store this key securely. It will not be shown again.".into(),
        }),
    ))
}

pub async fn update_api_key(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateApiKey>,
) -> Result<Json<ApiKeyResponse>, AppError> {
    let existing: shared::models::ApiKey =
        sqlx::query_as("SELECT * FROM api_keys WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("API key not found".into()))?;

    let name = body.name.unwrap_or(existing.name);
    let active = body.active.unwrap_or(existing.active);
    let expires_at = if body.expires_at.is_some() {
        body.expires_at
    } else {
        existing.expires_at
    };

    let updated: shared::models::ApiKey = sqlx::query_as(
        "UPDATE api_keys SET name = $1, active = $2, expires_at = $3, updated_at = now() WHERE id = $4 RETURNING *",
    )
    .bind(&name)
    .bind(active)
    .bind(expires_at)
    .bind(id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(ApiKeyResponse {
        id: updated.id,
        name: updated.name,
        route_id: updated.route_id,
        active: updated.active,
        expires_at: updated.expires_at,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
    }))
}

pub async fn delete_api_key(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("API key not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
