use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::SqliteApiKey;
use super::{ListResponse, PaginationParams};

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

impl From<shared::models::ApiKey> for ApiKeyResponse {
    fn from(k: shared::models::ApiKey) -> Self {
        Self {
            id: k.id,
            name: k.name,
            route_id: k.route_id,
            active: k.active,
            expires_at: k.expires_at,
            created_at: k.created_at,
            updated_at: k.updated_at,
        }
    }
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

pub async fn list_api_keys(
    State(pool): State<SqlitePool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<ApiKeyResponse>>, AppError> {
    let (page, limit, offset) = params.resolve();

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM api_keys")
        .fetch_one(&pool)
        .await?;

    let keys: Vec<SqliteApiKey> = sqlx::query_as(
        "SELECT * FROM api_keys ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let data = keys
        .into_iter()
        .map(|k| ApiKeyResponse::from(shared::models::ApiKey::from(k)))
        .collect();

    Ok(Json(ListResponse {
        data,
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_api_key(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateApiKey>,
) -> Result<(axum::http::StatusCode, Json<ApiKeyCreatedResponse>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }

    if let Some(route_id) = body.route_id {
        super::ensure_route_exists(&pool, route_id).await
            .map_err(|_| AppError::Validation("route_id does not exist".into()))?;
    }

    let random_bytes: [u8; 32] = rand::random();
    let plaintext_key = format!("gw_{}", hex::encode(random_bytes));

    let mut hasher = Sha256::new();
    hasher.update(plaintext_key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let id = Uuid::new_v4().to_string();
    let expires_at_str = body.expires_at.map(|dt| dt.to_rfc3339());

    let row: SqliteApiKey = sqlx::query_as(
        "INSERT INTO api_keys (id, name, key_hash, route_id, expires_at) VALUES (?1, ?2, ?3, ?4, ?5) RETURNING *",
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&key_hash)
    .bind(body.route_id.map(|r| r.to_string()))
    .bind(&expires_at_str)
    .fetch_one(&pool)
    .await?;

    let key: shared::models::ApiKey = row.into();

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
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateApiKey>,
) -> Result<Json<ApiKeyResponse>, AppError> {
    let existing_row: SqliteApiKey =
        sqlx::query_as("SELECT * FROM api_keys WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("API key not found".into()))?;
    let existing: shared::models::ApiKey = existing_row.into();

    let name = body.name.unwrap_or(existing.name);
    let active = body.active.unwrap_or(existing.active);
    let expires_at = if body.expires_at.is_some() {
        body.expires_at
    } else {
        existing.expires_at
    };

    let expires_at_str = expires_at.map(|dt| dt.to_rfc3339());

    let row: SqliteApiKey = sqlx::query_as(
        "UPDATE api_keys SET name = ?1, active = ?2, expires_at = ?3, updated_at = datetime('now') WHERE id = ?4 RETURNING *",
    )
    .bind(&name)
    .bind(active)
    .bind(&expires_at_str)
    .bind(id.to_string())
    .fetch_one(&pool)
    .await?;

    let updated: shared::models::ApiKey = row.into();
    Ok(Json(ApiKeyResponse::from(updated)))
}

pub async fn delete_api_key(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    super::delete_by_id(&pool, "api_keys", id, "API key").await
}
