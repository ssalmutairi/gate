use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Deserialize)]
pub struct CreateIpRule {
    pub cidr: String,
    pub action: String,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct IpRuleResponse {
    pub id: Uuid,
    pub route_id: Uuid,
    pub cidr: String,
    pub action: String,
    pub description: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_ip_rules(
    State(pool): State<PgPool>,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Vec<IpRuleResponse>>, AppError> {
    let rows: Vec<shared::models::IpRule> =
        sqlx::query_as("SELECT * FROM ip_rules WHERE route_id = $1 ORDER BY created_at")
            .bind(route_id)
            .fetch_all(&pool)
            .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| IpRuleResponse {
                id: r.id,
                route_id: r.route_id,
                cidr: r.cidr,
                action: r.action,
                description: r.description,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

pub async fn create_ip_rule(
    State(pool): State<PgPool>,
    Path(route_id): Path<Uuid>,
    Json(body): Json<CreateIpRule>,
) -> Result<(axum::http::StatusCode, Json<IpRuleResponse>), AppError> {
    if body.action != "allow" && body.action != "deny" {
        return Err(AppError::Validation(
            "action must be 'allow' or 'deny'".into(),
        ));
    }

    // Validate CIDR format
    if body.cidr.parse::<ipnet::IpNet>().is_err() {
        // Also allow bare IP addresses
        if body.cidr.parse::<std::net::IpAddr>().is_err() {
            return Err(AppError::Validation(
                "cidr must be a valid CIDR notation (e.g. 10.0.0.0/8) or IP address".into(),
            ));
        }
    }

    // Verify route exists
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM routes WHERE id = $1")
        .bind(route_id)
        .fetch_one(&pool)
        .await?;
    if count.0 == 0 {
        return Err(AppError::NotFound("Route not found".into()));
    }

    let description = body.description.unwrap_or_default();

    let rule: shared::models::IpRule = sqlx::query_as(
        "INSERT INTO ip_rules (route_id, cidr, action, description) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(route_id)
    .bind(&body.cidr)
    .bind(&body.action)
    .bind(&description)
    .fetch_one(&pool)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(IpRuleResponse {
            id: rule.id,
            route_id: rule.route_id,
            cidr: rule.cidr,
            action: rule.action,
            description: rule.description,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        }),
    ))
}

pub async fn delete_ip_rule(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM ip_rules WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("IP rule not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
