use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

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

pub async fn list_header_rules(
    State(pool): State<PgPool>,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Vec<HeaderRuleResponse>>, AppError> {
    let rows: Vec<shared::models::HeaderRule> =
        sqlx::query_as("SELECT * FROM header_rules WHERE route_id = $1 ORDER BY created_at")
            .bind(route_id)
            .fetch_all(&pool)
            .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| HeaderRuleResponse {
                id: r.id,
                route_id: r.route_id,
                phase: r.phase,
                action: r.action,
                header_name: r.header_name,
                header_value: r.header_value,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

pub async fn create_header_rule(
    State(pool): State<PgPool>,
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

    // Validate header name is a valid HTTP header name
    if body.header_name.len() > 256 {
        return Err(AppError::Validation("header_name must be 256 characters or fewer".into()));
    }
    if axum::http::header::HeaderName::from_bytes(body.header_name.trim().as_bytes()).is_err() {
        return Err(AppError::Validation("header_name is not a valid HTTP header name".into()));
    }

    // Reject CRLF in header values to prevent header injection
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

    // Verify route exists
    let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM routes WHERE id = $1")
        .bind(route_id)
        .fetch_one(&pool)
        .await
        .map_err(|_| AppError::NotFound("Route not found".into()))?;

    let rule: shared::models::HeaderRule = sqlx::query_as(
        "INSERT INTO header_rules (route_id, phase, action, header_name, header_value) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(route_id)
    .bind(&phase)
    .bind(&body.action)
    .bind(body.header_name.trim())
    .bind(&body.header_value)
    .fetch_one(&pool)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(HeaderRuleResponse {
            id: rule.id,
            route_id: rule.route_id,
            phase: rule.phase,
            action: rule.action,
            header_name: rule.header_name,
            header_value: rule.header_value,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        }),
    ))
}

pub async fn delete_header_rule(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM header_rules WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Header rule not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
