use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::SqliteIpRule;

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

impl From<shared::models::IpRule> for IpRuleResponse {
    fn from(r: shared::models::IpRule) -> Self {
        Self {
            id: r.id,
            route_id: r.route_id,
            cidr: r.cidr,
            action: r.action,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub async fn list_ip_rules(
    State(pool): State<SqlitePool>,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Vec<IpRuleResponse>>, AppError> {
    let rows: Vec<SqliteIpRule> =
        sqlx::query_as("SELECT * FROM ip_rules WHERE route_id = ?1 ORDER BY created_at")
            .bind(route_id.to_string())
            .fetch_all(&pool)
            .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| IpRuleResponse::from(shared::models::IpRule::from(r)))
            .collect(),
    ))
}

pub async fn create_ip_rule(
    State(pool): State<SqlitePool>,
    Path(route_id): Path<Uuid>,
    Json(body): Json<CreateIpRule>,
) -> Result<(axum::http::StatusCode, Json<IpRuleResponse>), AppError> {
    if body.action != "allow" && body.action != "deny" {
        return Err(AppError::Validation(
            "action must be 'allow' or 'deny'".into(),
        ));
    }

    if body.cidr.parse::<ipnet::IpNet>().is_err() {
        if body.cidr.parse::<std::net::IpAddr>().is_err() {
            return Err(AppError::Validation(
                "cidr must be a valid CIDR notation (e.g. 10.0.0.0/8) or IP address".into(),
            ));
        }
    }

    super::ensure_route_exists(&pool, route_id).await?;

    let description = body.description.unwrap_or_default();
    let id = Uuid::new_v4().to_string();

    let row: SqliteIpRule = sqlx::query_as(
        "INSERT INTO ip_rules (id, route_id, cidr, action, description) VALUES (?1, ?2, ?3, ?4, ?5) RETURNING *",
    )
    .bind(&id)
    .bind(route_id.to_string())
    .bind(&body.cidr)
    .bind(&body.action)
    .bind(&description)
    .fetch_one(&pool)
    .await?;

    let rule: shared::models::IpRule = row.into();
    Ok((
        axum::http::StatusCode::CREATED,
        Json(IpRuleResponse::from(rule)),
    ))
}

pub async fn delete_ip_rule(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    super::delete_by_id(&pool, "ip_rules", id, "IP rule").await
}
