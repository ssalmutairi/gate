use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

// --- DTOs ---

#[derive(Deserialize)]
pub struct ImportRequest {
    pub url: Option<String>,
    pub spec_content: Option<String>,
    pub namespace: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateService {
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct ServiceFilterParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub search: Option<String>,
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct ServiceResponse {
    pub id: Uuid,
    pub namespace: String,
    pub version: i32,
    pub spec_url: String,
    pub spec_hash: String,
    pub upstream_id: Uuid,
    pub route_id: Option<Uuid>,
    pub description: String,
    pub tags: Vec<String>,
    pub status: String,
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

/// Extract base path, host, port, and TLS from the first server URL in an OpenAPI spec.
fn parse_spec_server(spec: &serde_json::Value) -> Result<(String, String, u16, bool), AppError> {
    let server_url = spec
        .get("servers")
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
        .ok_or_else(|| AppError::Validation("Spec must have at least one server URL".into()))?;

    // Parse the server URL
    let parsed = url::Url::parse(server_url).map_err(|e| {
        AppError::Validation(format!("Invalid server URL '{}': {}", server_url, e))
    })?;

    let tls = parsed.scheme() == "https";
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::Validation("Server URL has no host".into()))?
        .to_string();
    let default_port = if tls { 443 } else { 80 };
    let port = parsed.port().unwrap_or(default_port);
    let base_path = parsed.path().trim_end_matches('/').to_string();

    Ok((base_path, host, port, tls))
}

// --- Helpers ---

/// Convert a friendly name like "Pet Store" into a URL-safe slug "pet-store".
fn slugify(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// --- Handlers ---

pub async fn import_service(
    State(pool): State<PgPool>,
    Json(body): Json<ImportRequest>,
) -> Result<(axum::http::StatusCode, Json<ServiceResponse>), AppError> {
    // Slugify namespace
    let namespace = slugify(&body.namespace);
    if namespace.is_empty() {
        return Err(AppError::Validation("namespace is required".into()));
    }

    // Acquire spec bytes and determine spec_url
    let (spec_bytes, spec_url): (Vec<u8>, String) =
        if let Some(ref content) = body.spec_content {
            let bytes = content.as_bytes().to_vec();
            let url = body.url.clone().unwrap_or_else(|| "inline".to_string());
            (bytes, url)
        } else if let Some(ref url) = body.url {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

            let resp = client
                .get(url)
                .send()
                .await
                .map_err(|e| AppError::Validation(format!("Failed to fetch spec: {}", e)))?;

            if !resp.status().is_success() {
                return Err(AppError::Validation(format!(
                    "Spec URL returned HTTP {}",
                    resp.status()
                )));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppError::Internal(format!("Failed to read spec body: {}", e)))?
                .to_vec();
            (bytes, url.clone())
        } else {
            return Err(AppError::Validation(
                "Either 'url' or 'spec_content' must be provided".into(),
            ));
        };

    // Compute hash
    let mut hasher = Sha256::new();
    hasher.update(&spec_bytes);
    let spec_hash = hex::encode(hasher.finalize());

    // Parse JSON
    let spec: serde_json::Value = serde_json::from_slice(&spec_bytes)
        .map_err(|e| AppError::Validation(format!("Invalid JSON in spec: {}", e)))?;

    // Extract server info
    let (base_path, host, port, tls) = parse_spec_server(&spec)?;

    // Check if namespace already exists
    let existing: Option<shared::models::Service> =
        sqlx::query_as("SELECT * FROM services WHERE namespace = $1")
            .bind(&namespace)
            .fetch_optional(&pool)
            .await?;

    if let Some(existing) = existing {
        if existing.spec_hash == spec_hash {
            return Err(AppError::Conflict(
                "No changes detected — spec hash is identical".into(),
            ));
        }

        // Version bump: update upstream targets, route prefix, service record
        let new_version = existing.version + 1;

        // Update/replace target on the upstream
        sqlx::query("DELETE FROM targets WHERE upstream_id = $1")
            .bind(existing.upstream_id)
            .execute(&pool)
            .await?;

        sqlx::query(
            "INSERT INTO targets (upstream_id, host, port, weight, tls) VALUES ($1, $2, $3, 1, $4)",
        )
        .bind(existing.upstream_id)
        .bind(&host)
        .bind(port as i32)
        .bind(tls)
        .execute(&pool)
        .await?;

        // Update route upstream_path_prefix if route still exists
        if let Some(route_id) = existing.route_id {
            let prefix_val = if base_path.is_empty() {
                None
            } else {
                Some(&base_path)
            };
            sqlx::query(
                "UPDATE routes SET upstream_path_prefix = $1, updated_at = now() WHERE id = $2",
            )
            .bind(prefix_val)
            .bind(route_id)
            .execute(&pool)
            .await?;
        }

        // Update service record
        let updated: shared::models::Service = sqlx::query_as(
            "UPDATE services SET version = $1, spec_url = $2, spec_hash = $3, updated_at = now() WHERE id = $4 RETURNING *",
        )
        .bind(new_version)
        .bind(&spec_url)
        .bind(&spec_hash)
        .bind(existing.id)
        .fetch_one(&pool)
        .await?;

        // Touch upstream to trigger hot reload
        sqlx::query("UPDATE upstreams SET updated_at = now() WHERE id = $1")
            .bind(existing.upstream_id)
            .execute(&pool)
            .await?;

        return Ok((
            axum::http::StatusCode::OK,
            Json(ServiceResponse {
                id: updated.id,
                namespace: updated.namespace,
                version: updated.version,
                spec_url: updated.spec_url,
                spec_hash: updated.spec_hash,
                upstream_id: updated.upstream_id,
                route_id: updated.route_id,
                description: updated.description,
                tags: updated.tags,
                status: updated.status,
                created_at: updated.created_at,
                updated_at: updated.updated_at,
            }),
        ));
    }

    // New namespace: create upstream, target, route, service
    let upstream: shared::models::Upstream = sqlx::query_as(
        "INSERT INTO upstreams (name, algorithm) VALUES ($1, 'round_robin') RETURNING *",
    )
    .bind(format!("svc-{}", namespace))
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO targets (upstream_id, host, port, weight, tls) VALUES ($1, $2, $3, 1, $4)",
    )
    .bind(upstream.id)
    .bind(&host)
    .bind(port as i32)
    .bind(tls)
    .execute(&pool)
    .await?;

    let path_prefix = format!("/{}", namespace);
    let upstream_path_prefix: Option<&str> = if base_path.is_empty() {
        None
    } else {
        Some(&base_path)
    };

    let route: shared::models::Route = sqlx::query_as(
        r#"INSERT INTO routes (name, path_prefix, upstream_id, strip_prefix, upstream_path_prefix)
           VALUES ($1, $2, $3, true, $4) RETURNING *"#,
    )
    .bind(format!("svc-{}", namespace))
    .bind(&path_prefix)
    .bind(upstream.id)
    .bind(upstream_path_prefix)
    .fetch_one(&pool)
    .await?;

    let description = body.description.unwrap_or_default();
    let tags = body.tags.unwrap_or_default();
    let status = body.status.unwrap_or_else(|| "stable".to_string());

    let service: shared::models::Service = sqlx::query_as(
        r#"INSERT INTO services (namespace, spec_url, spec_hash, upstream_id, route_id, description, tags, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *"#,
    )
    .bind(&namespace)
    .bind(&spec_url)
    .bind(&spec_hash)
    .bind(upstream.id)
    .bind(route.id)
    .bind(&description)
    .bind(&tags)
    .bind(&status)
    .fetch_one(&pool)
    .await?;

    // Set service_id on route
    sqlx::query("UPDATE routes SET service_id = $1, updated_at = now() WHERE id = $2")
        .bind(service.id)
        .bind(route.id)
        .execute(&pool)
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ServiceResponse {
            id: service.id,
            namespace: service.namespace,
            version: service.version,
            spec_url: service.spec_url,
            spec_hash: service.spec_hash,
            upstream_id: service.upstream_id,
            route_id: service.route_id,
            description: service.description,
            tags: service.tags,
            status: service.status,
            created_at: service.created_at,
            updated_at: service.updated_at,
        }),
    ))
}

pub async fn list_services(
    State(pool): State<PgPool>,
    Query(params): Query<ServiceFilterParams>,
) -> Result<Json<ListResponse<ServiceResponse>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    // Build dynamic WHERE clause
    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx = 1u32;

    if params.search.is_some() {
        conditions.push(format!("namespace ILIKE ${param_idx}"));
        param_idx += 1;
    }
    if params.status.is_some() {
        conditions.push(format!("status = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM services {where_clause}");
    let list_sql = format!(
        "SELECT * FROM services {where_clause} ORDER BY created_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );

    // Build count query
    let mut count_query = sqlx::query_as::<_, (i64,)>(&count_sql);
    if let Some(ref search) = params.search {
        count_query = count_query.bind(format!("%{search}%"));
    }
    if let Some(ref status) = params.status {
        count_query = count_query.bind(status);
    }
    let total: (i64,) = count_query.fetch_one(&pool).await?;

    // Build list query
    let mut list_query = sqlx::query_as::<_, shared::models::Service>(&list_sql);
    if let Some(ref search) = params.search {
        list_query = list_query.bind(format!("%{search}%"));
    }
    if let Some(ref status) = params.status {
        list_query = list_query.bind(status);
    }
    list_query = list_query.bind(limit).bind(offset);

    let rows: Vec<shared::models::Service> = list_query.fetch_all(&pool).await?;

    Ok(Json(ListResponse {
        data: rows
            .into_iter()
            .map(|s| ServiceResponse {
                id: s.id,
                namespace: s.namespace,
                version: s.version,
                spec_url: s.spec_url,
                spec_hash: s.spec_hash,
                upstream_id: s.upstream_id,
                route_id: s.route_id,
                description: s.description,
                tags: s.tags,
                status: s.status,
                created_at: s.created_at,
                updated_at: s.updated_at,
            })
            .collect(),
        total: total.0,
        page,
        limit,
    }))
}

pub async fn get_service(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<ServiceResponse>, AppError> {
    let service: shared::models::Service =
        sqlx::query_as("SELECT * FROM services WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    Ok(Json(ServiceResponse {
        id: service.id,
        namespace: service.namespace,
        version: service.version,
        spec_url: service.spec_url,
        spec_hash: service.spec_hash,
        upstream_id: service.upstream_id,
        route_id: service.route_id,
        description: service.description,
        tags: service.tags,
        status: service.status,
        created_at: service.created_at,
        updated_at: service.updated_at,
    }))
}

pub async fn update_service(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateService>,
) -> Result<Json<ServiceResponse>, AppError> {
    let existing: shared::models::Service =
        sqlx::query_as("SELECT * FROM services WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    let description = body.description.unwrap_or(existing.description);
    let tags = body.tags.unwrap_or(existing.tags);
    let status = body.status.unwrap_or(existing.status);

    if !matches!(status.as_str(), "alpha" | "beta" | "stable" | "deprecated") {
        return Err(AppError::Validation(
            "status must be 'alpha', 'beta', 'stable', or 'deprecated'".into(),
        ));
    }

    let updated: shared::models::Service = sqlx::query_as(
        "UPDATE services SET description = $1, tags = $2, status = $3, updated_at = now() WHERE id = $4 RETURNING *",
    )
    .bind(&description)
    .bind(&tags)
    .bind(&status)
    .bind(id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(ServiceResponse {
        id: updated.id,
        namespace: updated.namespace,
        version: updated.version,
        spec_url: updated.spec_url,
        spec_hash: updated.spec_hash,
        upstream_id: updated.upstream_id,
        route_id: updated.route_id,
        description: updated.description,
        tags: updated.tags,
        status: updated.status,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
    }))
}

pub async fn delete_service(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let service: shared::models::Service =
        sqlx::query_as("SELECT * FROM services WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    // Delete route if it exists
    if let Some(route_id) = service.route_id {
        sqlx::query("DELETE FROM routes WHERE id = $1")
            .bind(route_id)
            .execute(&pool)
            .await?;
    }

    // Delete upstream (cascades to targets via FK)
    sqlx::query("DELETE FROM upstreams WHERE id = $1")
        .bind(service.upstream_id)
        .execute(&pool)
        .await?;

    // Delete service record (may already be gone from cascade, ignore errors)
    let _ = sqlx::query("DELETE FROM services WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
