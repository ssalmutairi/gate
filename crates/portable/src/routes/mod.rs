pub mod api_keys;
pub mod compositions;
pub mod header_rules;
pub mod health;
pub mod ip_rules;
pub mod rate_limits;
pub mod routes;
pub mod services;
pub mod upstreams;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

#[derive(Deserialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

impl PaginationParams {
    pub fn resolve(&self) -> (i64, i64, i64) {
        let page = self.page.unwrap_or(1).max(1);
        let limit = self.limit.unwrap_or(20).clamp(1, 100);
        let offset = (page - 1) * limit;
        (page, limit, offset)
    }
}

const ALLOWED_TABLES: &[&str] = &[
    "upstreams", "routes", "api_keys", "rate_limits",
    "header_rules", "ip_rules", "services", "compositions",
    "composition_steps",
];

pub async fn delete_by_id(
    pool: &SqlitePool,
    table: &str,
    id: Uuid,
    entity_name: &str,
) -> Result<axum::http::StatusCode, AppError> {
    debug_assert!(
        ALLOWED_TABLES.contains(&table),
        "delete_by_id called with unknown table: {}",
        table
    );
    let sql = format!("DELETE FROM {} WHERE id = ?1", table);
    let result = sqlx::query(&sql)
        .bind(id.to_string())
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("{} not found", entity_name)));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn ensure_route_exists(pool: &SqlitePool, route_id: Uuid) -> Result<(), AppError> {
    sqlx::query("SELECT id FROM routes WHERE id = ?1")
        .bind(route_id.to_string())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Route not found".into()))?;
    Ok(())
}
