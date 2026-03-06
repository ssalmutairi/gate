use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::SqliteHeaderRule;

#[derive(Deserialize)]
pub struct CreateHeaderRule {
    pub phase: Option<String>,
    pub action: String,
    pub header_name: String,
    pub header_value: Option<String>,
}

#[derive(Serialize)]
pub struct HeaderRuleResponse {
    pub id: Uuid,
    pub route_id: Uuid,
    pub phase: String,
    pub action: String,
    pub header_name: String,
    pub header_value: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<shared::models::HeaderRule> for HeaderRuleResponse {
    fn from(r: shared::models::HeaderRule) -> Self {
        Self {
            id: r.id,
            route_id: r.route_id,
            phase: r.phase,
            action: r.action,
            header_name: r.header_name,
            header_value: r.header_value,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn list_header_rules(
    State(pool): State<SqlitePool>,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Vec<HeaderRuleResponse>>, AppError> {
    let rows: Vec<SqliteHeaderRule> =
        sqlx::query_as("SELECT * FROM header_rules WHERE route_id = ?1 ORDER BY created_at")
            .bind(route_id.to_string())
            .fetch_all(&pool)
            .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| HeaderRuleResponse::from(shared::models::HeaderRule::from(r)))
            .collect(),
    ))
}

pub async fn create_header_rule(
    State(pool): State<SqlitePool>,
    Path(route_id): Path<Uuid>,
    Json(body): Json<CreateHeaderRule>,
) -> Result<(axum::http::StatusCode, Json<HeaderRuleResponse>), AppError> {
    let phase = body.phase.unwrap_or_else(|| "request".to_string());
    if phase != "request" && phase != "response" {
        return Err(AppError::Validation(
            "phase must be 'request' or 'response'".into(),
        ));
    }

    let action = body.action.as_str();
    if !matches!(action, "set" | "add" | "remove") {
        return Err(AppError::Validation(
            "action must be 'set', 'add', or 'remove'".into(),
        ));
    }

    if action != "remove" && body.header_value.is_none() {
        return Err(AppError::Validation(
            "header_value is required for 'set' and 'add' actions".into(),
        ));
    }

    if body.header_name.trim().is_empty() {
        return Err(AppError::Validation("header_name is required".into()));
    }

    if body.header_name.len() > 256 {
        return Err(AppError::Validation("header_name must be 256 characters or fewer".into()));
    }
    if axum::http::header::HeaderName::from_bytes(body.header_name.trim().as_bytes()).is_err() {
        return Err(AppError::Validation("header_name is not a valid HTTP header name".into()));
    }

    if let Some(ref val) = body.header_value {
        if val.contains('\r') || val.contains('\n') {
            return Err(AppError::Validation(
                "header_value must not contain CR or LF characters".into(),
            ));
        }
        if val.len() > 8192 {
            return Err(AppError::Validation("header_value must be 8192 characters or fewer".into()));
        }
    }

    super::ensure_route_exists(&pool, route_id).await?;

    let id = Uuid::new_v4().to_string();
    let row: SqliteHeaderRule = sqlx::query_as(
        "INSERT INTO header_rules (id, route_id, phase, action, header_name, header_value) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING *",
    )
    .bind(&id)
    .bind(route_id.to_string())
    .bind(&phase)
    .bind(&body.action)
    .bind(body.header_name.trim())
    .bind(&body.header_value)
    .fetch_one(&pool)
    .await?;

    let rule: shared::models::HeaderRule = row.into();
    Ok((
        axum::http::StatusCode::CREATED,
        Json(HeaderRuleResponse::from(rule)),
    ))
}

pub async fn delete_header_rule(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    super::delete_by_id(&pool, "header_rules", id, "Header rule").await
}
