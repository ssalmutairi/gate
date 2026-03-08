use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::SqliteUpstream;
use crate::models::SqliteTarget;
use super::{ListResponse, PaginationParams};

#[derive(Deserialize)]
pub struct CreateUpstream {
    pub name: String,
    pub algorithm: Option<String>,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_duration_secs: Option<i32>,
    pub tls_ca_cert: Option<String>,
    pub tls_client_cert: Option<String>,
    pub tls_client_key: Option<String>,
    pub tls_skip_verify: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateUpstream {
    pub name: Option<String>,
    pub algorithm: Option<String>,
    pub active: Option<bool>,
    pub circuit_breaker_threshold: Option<Option<i32>>,
    pub circuit_breaker_duration_secs: Option<i32>,
    pub tls_ca_cert: Option<Option<String>>,
    pub tls_client_cert: Option<Option<String>>,
    pub tls_client_key: Option<Option<String>>,
    pub tls_skip_verify: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateTarget {
    pub host: String,
    pub port: i32,
    pub weight: Option<i32>,
    pub tls: Option<bool>,
}

#[derive(Serialize)]
pub struct UpstreamResponse {
    pub id: Uuid,
    pub name: String,
    pub algorithm: String,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_duration_secs: i32,
    pub active: bool,
    pub tls_ca_cert: Option<String>,
    pub tls_client_cert: Option<String>,
    pub tls_client_key: Option<String>,
    pub tls_skip_verify: bool,
    pub targets: Vec<TargetResponse>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl UpstreamResponse {
    pub fn from_upstream(u: shared::models::Upstream, targets: Vec<TargetResponse>) -> Self {
        Self {
            id: u.id,
            name: u.name,
            algorithm: u.algorithm,
            circuit_breaker_threshold: u.circuit_breaker_threshold,
            circuit_breaker_duration_secs: u.circuit_breaker_duration_secs,
            active: u.active,
            tls_ca_cert: u.tls_ca_cert,
            tls_client_cert: u.tls_client_cert,
            tls_client_key: u.tls_client_key,
            tls_skip_verify: u.tls_skip_verify,
            targets,
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct TargetResponse {
    pub id: Uuid,
    pub upstream_id: Uuid,
    pub host: String,
    pub port: i32,
    pub weight: i32,
    pub healthy: bool,
    pub tls: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<SqliteTarget> for TargetResponse {
    fn from(t: SqliteTarget) -> Self {
        Self {
            id: crate::models::parse_uuid(&t.id),
            upstream_id: crate::models::parse_uuid(&t.upstream_id),
            host: t.host,
            port: t.port,
            weight: t.weight,
            healthy: t.healthy,
            tls: t.tls,
            created_at: crate::models::parse_dt(&t.created_at),
            updated_at: crate::models::parse_dt(&t.updated_at),
        }
    }
}

fn validate_algorithm(algorithm: &str) -> Result<(), AppError> {
    if !matches!(algorithm, "round_robin" | "weighted_round_robin" | "least_connections") {
        return Err(AppError::Validation(
            "algorithm must be 'round_robin', 'weighted_round_robin', or 'least_connections'".into(),
        ));
    }
    Ok(())
}

fn validate_circuit_breaker(threshold: Option<i32>, duration_secs: i32) -> Result<(), AppError> {
    if let Some(t) = threshold {
        if t < 1 || t > 100 {
            return Err(AppError::Validation("circuit_breaker_threshold must be between 1 and 100".into()));
        }
    }
    if duration_secs < 5 || duration_secs > 3600 {
        return Err(AppError::Validation("circuit_breaker_duration_secs must be between 5 and 3600".into()));
    }
    Ok(())
}

pub async fn list_upstreams(
    State(pool): State<SqlitePool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<UpstreamResponse>>, AppError> {
    let (page, limit, offset) = params.resolve();

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM upstreams")
        .fetch_one(&pool)
        .await?;

    let upstreams: Vec<SqliteUpstream> =
        sqlx::query_as("SELECT * FROM upstreams ORDER BY created_at DESC LIMIT ?1 OFFSET ?2")
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?;

    // Batch-fetch targets for all upstreams on this page to avoid N+1
    let upstream_ids: Vec<String> = upstreams.iter().map(|u| u.id.clone()).collect();
    let all_targets: Vec<SqliteTarget> = if upstream_ids.is_empty() {
        vec![]
    } else {
        let placeholders: String = upstream_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT * FROM targets WHERE upstream_id IN ({}) ORDER BY created_at",
            placeholders
        );
        let mut query = sqlx::query_as::<_, SqliteTarget>(&sql);
        for id in &upstream_ids {
            query = query.bind(id);
        }
        query.fetch_all(&pool).await?
    };

    // Group targets by upstream_id
    let mut targets_by_upstream: std::collections::HashMap<String, Vec<TargetResponse>> =
        std::collections::HashMap::new();
    for t in all_targets {
        targets_by_upstream
            .entry(t.upstream_id.clone())
            .or_default()
            .push(TargetResponse::from(t));
    }

    let results: Vec<UpstreamResponse> = upstreams
        .into_iter()
        .map(|u| {
            let id = u.id.clone();
            let upstream: shared::models::Upstream = u.into();
            let targets = targets_by_upstream.remove(&id).unwrap_or_default();
            UpstreamResponse::from_upstream(upstream, targets)
        })
        .collect();

    Ok(Json(ListResponse {
        data: results,
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_upstream(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateUpstream>,
) -> Result<(axum::http::StatusCode, Json<UpstreamResponse>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if body.name.len() > 255 {
        return Err(AppError::Validation("name must be 255 characters or fewer".into()));
    }

    let algorithm = body.algorithm.unwrap_or_else(|| "round_robin".into());
    validate_algorithm(&algorithm)?;
    let cb_duration = body.circuit_breaker_duration_secs.unwrap_or(30);
    validate_circuit_breaker(body.circuit_breaker_threshold, cb_duration)?;

    // Validate mTLS: cert and key must be provided together
    if body.tls_client_cert.is_some() != body.tls_client_key.is_some() {
        return Err(AppError::Validation("tls_client_cert and tls_client_key must both be provided".into()));
    }

    let tls_skip_verify = body.tls_skip_verify.unwrap_or(false);

    let id = Uuid::new_v4().to_string();
    let row: SqliteUpstream = sqlx::query_as(
        "INSERT INTO upstreams (id, name, algorithm, circuit_breaker_threshold, circuit_breaker_duration_secs, tls_ca_cert, tls_client_cert, tls_client_key, tls_skip_verify) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING *",
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&algorithm)
    .bind(body.circuit_breaker_threshold)
    .bind(cb_duration)
    .bind(&body.tls_ca_cert)
    .bind(&body.tls_client_cert)
    .bind(&body.tls_client_key)
    .bind(tls_skip_verify)
    .fetch_one(&pool)
    .await?;

    let upstream: shared::models::Upstream = row.into();
    Ok((
        axum::http::StatusCode::CREATED,
        Json(UpstreamResponse::from_upstream(upstream, vec![])),
    ))
}

pub async fn get_upstream(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<Json<UpstreamResponse>, AppError> {
    let row: SqliteUpstream =
        sqlx::query_as("SELECT * FROM upstreams WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Upstream not found".into()))?;

    let targets: Vec<SqliteTarget> =
        sqlx::query_as("SELECT * FROM targets WHERE upstream_id = ?1 ORDER BY created_at")
            .bind(id.to_string())
            .fetch_all(&pool)
            .await?;

    let upstream: shared::models::Upstream = row.into();
    Ok(Json(UpstreamResponse::from_upstream(
        upstream,
        targets.into_iter().map(TargetResponse::from).collect(),
    )))
}

pub async fn update_upstream(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUpstream>,
) -> Result<Json<UpstreamResponse>, AppError> {
    let existing_row: SqliteUpstream =
        sqlx::query_as("SELECT * FROM upstreams WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Upstream not found".into()))?;
    let existing: shared::models::Upstream = existing_row.into();

    let name = body.name.unwrap_or(existing.name);
    let algorithm = body.algorithm.unwrap_or(existing.algorithm);
    let active = body.active.unwrap_or(existing.active);
    let circuit_breaker_threshold = if let Some(cbt) = body.circuit_breaker_threshold {
        cbt
    } else {
        existing.circuit_breaker_threshold
    };
    let circuit_breaker_duration_secs = body
        .circuit_breaker_duration_secs
        .unwrap_or(existing.circuit_breaker_duration_secs);

    validate_algorithm(&algorithm)?;
    validate_circuit_breaker(circuit_breaker_threshold, circuit_breaker_duration_secs)?;

    let tls_ca_cert = if let Some(v) = body.tls_ca_cert { v } else { existing.tls_ca_cert };
    let tls_client_cert = if let Some(v) = body.tls_client_cert { v } else { existing.tls_client_cert };
    let tls_client_key = if let Some(v) = body.tls_client_key { v } else { existing.tls_client_key };
    let tls_skip_verify = body.tls_skip_verify.unwrap_or(existing.tls_skip_verify);

    // Validate mTLS: cert and key must be provided together
    if tls_client_cert.is_some() != tls_client_key.is_some() {
        return Err(AppError::Validation("tls_client_cert and tls_client_key must both be provided".into()));
    }

    let row: SqliteUpstream = sqlx::query_as(
        "UPDATE upstreams SET name = ?1, algorithm = ?2, active = ?3, circuit_breaker_threshold = ?4, circuit_breaker_duration_secs = ?5, tls_ca_cert = ?6, tls_client_cert = ?7, tls_client_key = ?8, tls_skip_verify = ?9, updated_at = datetime('now') WHERE id = ?10 RETURNING *",
    )
    .bind(&name)
    .bind(&algorithm)
    .bind(active)
    .bind(circuit_breaker_threshold)
    .bind(circuit_breaker_duration_secs)
    .bind(&tls_ca_cert)
    .bind(&tls_client_cert)
    .bind(&tls_client_key)
    .bind(tls_skip_verify)
    .bind(id.to_string())
    .fetch_one(&pool)
    .await?;

    let targets: Vec<SqliteTarget> =
        sqlx::query_as("SELECT * FROM targets WHERE upstream_id = ?1 ORDER BY created_at")
            .bind(id.to_string())
            .fetch_all(&pool)
            .await?;

    let updated: shared::models::Upstream = row.into();
    Ok(Json(UpstreamResponse::from_upstream(
        updated,
        targets.into_iter().map(TargetResponse::from).collect(),
    )))
}

pub async fn delete_upstream(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    super::delete_by_id(&pool, "upstreams", id, "Upstream").await
}

pub async fn add_target(
    State(pool): State<SqlitePool>,
    Path(upstream_id): Path<Uuid>,
    Json(body): Json<CreateTarget>,
) -> Result<(axum::http::StatusCode, Json<TargetResponse>), AppError> {
    sqlx::query("SELECT id FROM upstreams WHERE id = ?1")
        .bind(upstream_id.to_string())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Upstream not found".into()))?;

    if body.host.trim().is_empty() {
        return Err(AppError::Validation("host is required".into()));
    }
    if body.host.len() > 255 {
        return Err(AppError::Validation("host must be 255 characters or fewer".into()));
    }
    let host = body.host.trim();
    if host.contains(|c: char| c.is_whitespace() || c == '/' || c == '\\' || c == '@') {
        return Err(AppError::Validation(
            "host contains invalid characters (no spaces, slashes, or @ allowed)".into(),
        ));
    }
    if body.port <= 0 || body.port > 65535 {
        return Err(AppError::Validation("port must be between 1 and 65535".into()));
    }

    let weight = body.weight.unwrap_or(1);
    let tls = body.tls.unwrap_or(false);
    let target_id = Uuid::new_v4().to_string();

    let target: SqliteTarget = sqlx::query_as(
        "INSERT INTO targets (id, upstream_id, host, port, weight, tls) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING *",
    )
    .bind(&target_id)
    .bind(upstream_id.to_string())
    .bind(host)
    .bind(body.port)
    .bind(weight)
    .bind(tls)
    .fetch_one(&pool)
    .await?;

    sqlx::query("UPDATE upstreams SET updated_at = datetime('now') WHERE id = ?1")
        .bind(upstream_id.to_string())
        .execute(&pool)
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(TargetResponse::from(target))))
}

pub async fn delete_target(
    State(pool): State<SqlitePool>,
    Path((upstream_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, AppError> {
    let result =
        sqlx::query("DELETE FROM targets WHERE id = ?1 AND upstream_id = ?2")
            .bind(target_id.to_string())
            .bind(upstream_id.to_string())
            .execute(&pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Target not found".into()));
    }

    sqlx::query("UPDATE upstreams SET updated_at = datetime('now') WHERE id = ?1")
        .bind(upstream_id.to_string())
        .execute(&pool)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
