use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::net::IpAddr;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{SqliteService, SqliteUpstream, SqliteRoute};
use super::{ListResponse, PaginationParams};

/// Shared config values available to handlers via Extension.
#[derive(Clone)]
pub struct AppSettings {
    pub max_spec_size_bytes: usize,
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified()
        }
    }
}

async fn validate_url_ssrf(url_str: &str) -> Result<Vec<std::net::SocketAddr>, AppError> {
    let parsed = url::Url::parse(url_str)
        .map_err(|e| AppError::Validation(format!("Invalid URL: {}", e)))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::Validation(
            "Only http and https URLs are allowed".into(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::Validation("URL has no host".into()))?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            return Err(AppError::Validation(
                "URL resolves to a private/reserved IP address".into(),
            ));
        }
        let port = parsed.port().unwrap_or(if scheme == "https" { 443 } else { 80 });
        return Ok(vec![std::net::SocketAddr::new(ip, port)]);
    }

    let port = parsed.port().unwrap_or(if scheme == "https" { 443 } else { 80 });
    let addrs: Vec<std::net::SocketAddr> =
        tokio::net::lookup_host(format!("{}:{}", host, port))
            .await
            .map_err(|e| AppError::Validation(format!("DNS resolution failed: {}", e)))?
            .collect();

    if addrs.is_empty() {
        return Err(AppError::Validation("DNS resolution returned no addresses".into()));
    }

    for addr in &addrs {
        if is_private_ip(&addr.ip()) {
            return Err(AppError::Validation(
                "URL resolves to a private/reserved IP address".into(),
            ));
        }
    }

    Ok(addrs)
}

#[derive(Deserialize)]
pub struct ImportRequest {
    pub url: Option<String>,
    pub spec_content: Option<String>,
    pub namespace: String,
    pub server_url: Option<String>,
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
    #[serde(flatten)]
    pub pagination: PaginationParams,
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
    pub service_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn service_to_response(s: shared::models::Service) -> ServiceResponse {
    ServiceResponse {
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
        service_type: s.service_type,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

fn parse_spec_server(
    spec: &serde_json::Value,
    source_url: &str,
    override_url: Option<&str>,
) -> Result<(String, String, u16, bool), AppError> {
    let server_url_string;
    let server_url = if let Some(url) = spec
        .get("servers")
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
    {
        url
    } else if let Some(host) = spec.get("host").and_then(|h| h.as_str()) {
        let scheme = spec
            .get("schemes")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .and_then(|s| s.as_str())
            .unwrap_or("https");
        let base_path = spec
            .get("basePath")
            .and_then(|b| b.as_str())
            .unwrap_or("");
        server_url_string = format!("{}://{}{}", scheme, host, base_path);
        &server_url_string
    } else if let Some(url) = override_url.filter(|u| !u.is_empty()) {
        url
    } else {
        return Err(AppError::Validation(
            "Spec has no server URL. Provide a server_url or use a spec with servers[] (OpenAPI 3.x) or host (Swagger 2.0).".into(),
        ));
    };

    let parsed = match url::Url::parse(server_url) {
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let base = url::Url::parse(source_url).map_err(|e| {
                AppError::Validation(format!(
                    "Server URL '{}' is relative but source URL '{}' is invalid: {}",
                    server_url, source_url, e
                ))
            })?;
            base.join(server_url).map_err(|e| {
                AppError::Validation(format!(
                    "Failed to resolve relative server URL '{}' against '{}': {}",
                    server_url, source_url, e
                ))
            })?
        }
        Err(e) => {
            return Err(AppError::Validation(format!(
                "Invalid server URL '{}': {}",
                server_url, e
            )));
        }
    };

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

pub async fn import_service(
    State(pool): State<SqlitePool>,
    Extension(settings): Extension<AppSettings>,
    Json(body): Json<ImportRequest>,
) -> Result<(axum::http::StatusCode, Json<ServiceResponse>), AppError> {
    let max_spec_bytes = settings.max_spec_size_bytes;
    if body.namespace.len() > 255 {
        return Err(AppError::Validation("namespace must be 255 characters or fewer".into()));
    }
    if let Some(ref desc) = body.description {
        if desc.len() > 2000 {
            return Err(AppError::Validation("description must be 2000 characters or fewer".into()));
        }
    }
    if let Some(ref url) = body.url {
        if url.len() > 2048 {
            return Err(AppError::Validation("url must be 2048 characters or fewer".into()));
        }
    }

    let namespace = slugify(&body.namespace);
    if namespace.is_empty() {
        return Err(AppError::Validation("namespace is required".into()));
    }

    let (spec_bytes, spec_url): (Vec<u8>, String) =
        if let Some(ref content) = body.spec_content {
            let bytes = content.as_bytes().to_vec();
            let url = body.url.clone().unwrap_or_else(|| "inline".to_string());
            (bytes, url)
        } else if let Some(ref url) = body.url {
            // Validate SSRF and get resolved addresses to pin
            let resolved_addrs = validate_url_ssrf(url).await?;

            let parsed = url::Url::parse(url)
                .map_err(|e| AppError::Validation(format!("Invalid URL: {}", e)))?;
            let host = parsed.host_str().unwrap_or_default().to_string();

            // Pin resolved IPs to prevent DNS rebinding TOCTOU
            let mut client_builder = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(5));
            for addr in &resolved_addrs {
                client_builder = client_builder.resolve(&host, *addr);
            }
            let client = client_builder
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

            if let Some(cl) = resp.content_length() {
                if cl as usize > max_spec_bytes {
                    return Err(AppError::Validation(format!(
                        "Spec response too large ({} bytes, max {})",
                        cl, max_spec_bytes
                    )));
                }
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppError::Internal(format!("Failed to read spec body: {}", e)))?;

            if bytes.len() > max_spec_bytes {
                return Err(AppError::Validation(format!(
                    "Spec response too large ({} bytes, max {})",
                    bytes.len(),
                    max_spec_bytes
                )));
            }

            (bytes.to_vec(), url.clone())
        } else {
            return Err(AppError::Validation(
                "Either 'url' or 'spec_content' must be provided".into(),
            ));
        };

    let mut hasher = Sha256::new();
    hasher.update(&spec_bytes);
    let spec_hash = hex::encode(hasher.finalize());

    let is_wsdl = crate::wsdl::is_wsdl(&spec_bytes);

    let (spec, spec_text, base_path, host, port, tls, service_type, soap_metadata): (
        serde_json::Value, String, String, String, u16, bool, &str, Option<serde_json::Value>,
    ) = if is_wsdl {
        let xml_str = String::from_utf8_lossy(&spec_bytes).to_string();
        let wsdl_result = crate::wsdl::parse_wsdl(&xml_str)
            .map_err(|e| AppError::Validation(format!("WSDL parse error: {}", e)))?;

        let parsed = url::Url::parse(&wsdl_result.endpoint_url).map_err(|e| {
            AppError::Validation(format!("Invalid SOAP endpoint URL: {}", e))
        })?;
        let tls = parsed.scheme() == "https";
        let host = parsed
            .host_str()
            .ok_or_else(|| AppError::Validation("SOAP endpoint has no host".into()))?
            .to_string();
        let default_port = if tls { 443 } else { 80 };
        let port = parsed.port().unwrap_or(default_port);
        let base_path = parsed.path().trim_end_matches('/').to_string();

        let spec_text = serde_json::to_string_pretty(&wsdl_result.openapi_spec)
            .unwrap_or_default();

        (
            wsdl_result.openapi_spec,
            spec_text,
            base_path,
            host,
            port,
            tls,
            "soap",
            Some(wsdl_result.soap_metadata),
        )
    } else {
        let spec: serde_json::Value = serde_json::from_slice(&spec_bytes)
            .map_err(|e| AppError::Validation(format!("Invalid JSON in spec: {}", e)))?;

        let spec_text = String::from_utf8_lossy(&spec_bytes).to_string();

        let (base_path, host, port, tls) =
            parse_spec_server(&spec, &spec_url, body.server_url.as_deref())?;

        (spec, spec_text, base_path, host, port, tls, "rest", None)
    };

    let has_paths = spec
        .get("paths")
        .and_then(|p| p.as_object())
        .is_some_and(|p| !p.is_empty());
    if !has_paths {
        return Err(AppError::Validation(
            "Spec has no endpoints — paths object is empty or missing".into(),
        ));
    }

    // Check if namespace already exists
    let existing: Option<SqliteService> =
        sqlx::query_as("SELECT * FROM services WHERE namespace = ?1")
            .bind(&namespace)
            .fetch_optional(&pool)
            .await?;

    if let Some(existing_row) = existing {
        let existing: shared::models::Service = existing_row.into();
        if existing.spec_hash == spec_hash {
            return Err(AppError::Conflict(
                "No changes detected — spec hash is identical".into(),
            ));
        }

        let new_version = existing.version + 1;

        // Use a transaction for multi-table update
        let mut tx = pool.begin().await?;

        sqlx::query("DELETE FROM targets WHERE upstream_id = ?1")
            .bind(existing.upstream_id.to_string())
            .execute(&mut *tx)
            .await?;

        let target_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO targets (id, upstream_id, host, port, weight, tls) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        )
        .bind(&target_id)
        .bind(existing.upstream_id.to_string())
        .bind(&host)
        .bind(port as i32)
        .bind(tls)
        .execute(&mut *tx)
        .await?;

        if let Some(route_id) = existing.route_id {
            let prefix_val = if base_path.is_empty() {
                None
            } else {
                Some(&base_path)
            };
            if service_type == "soap" {
                let methods_json = serde_json::to_string(&["POST"]).unwrap();
                sqlx::query(
                    "UPDATE routes SET upstream_path_prefix = ?1, methods = ?2, updated_at = datetime('now') WHERE id = ?3",
                )
                .bind(prefix_val)
                .bind(&methods_json)
                .bind(route_id.to_string())
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query(
                    "UPDATE routes SET upstream_path_prefix = ?1, updated_at = datetime('now') WHERE id = ?2",
                )
                .bind(prefix_val)
                .bind(route_id.to_string())
                .execute(&mut *tx)
                .await?;
            }
        }

        let soap_meta_str = soap_metadata.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        let description = body.description.as_deref().unwrap_or(&existing.description);
        let existing_tags_json = serde_json::to_string(&existing.tags).unwrap_or_else(|_| "[]".to_string());
        let tags_val = body.tags.as_ref()
            .map(|t| serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or(existing_tags_json);
        let status = body.status.as_deref().unwrap_or(&existing.status);

        let updated_row: SqliteService = sqlx::query_as(
            "UPDATE services SET version = ?1, spec_url = ?2, spec_hash = ?3, spec_content = ?4, service_type = ?5, soap_metadata = ?6, description = ?7, tags = ?8, status = ?9, updated_at = datetime('now') WHERE id = ?10 RETURNING *",
        )
        .bind(new_version)
        .bind(&spec_url)
        .bind(&spec_hash)
        .bind(&spec_text)
        .bind(service_type)
        .bind(&soap_meta_str)
        .bind(description)
        .bind(&tags_val)
        .bind(status)
        .bind(existing.id.to_string())
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query("UPDATE upstreams SET updated_at = datetime('now') WHERE id = ?1")
            .bind(existing.upstream_id.to_string())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        let updated: shared::models::Service = updated_row.into();
        return Ok((
            axum::http::StatusCode::OK,
            Json(service_to_response(updated)),
        ));
    }

    // New namespace — use a transaction
    let mut tx = pool.begin().await?;

    let upstream_id = Uuid::new_v4().to_string();
    let _: SqliteUpstream = sqlx::query_as(
        "INSERT INTO upstreams (id, name, algorithm) VALUES (?1, ?2, 'round_robin') RETURNING *",
    )
    .bind(&upstream_id)
    .bind(format!("svc-{}", namespace))
    .fetch_one(&mut *tx)
    .await?;

    let target_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO targets (id, upstream_id, host, port, weight, tls) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
    )
    .bind(&target_id)
    .bind(&upstream_id)
    .bind(&host)
    .bind(port as i32)
    .bind(tls)
    .execute(&mut *tx)
    .await?;

    let path_prefix = format!("/{}", namespace);
    let upstream_path_prefix: Option<&str> = if base_path.is_empty() {
        None
    } else {
        Some(&base_path)
    };

    let methods_json: Option<String> = if service_type == "soap" {
        Some(serde_json::to_string(&["POST"]).unwrap())
    } else {
        None
    };

    let route_id = Uuid::new_v4().to_string();
    let _: SqliteRoute = sqlx::query_as(
        r#"INSERT INTO routes (id, name, path_prefix, methods, upstream_id, strip_prefix, upstream_path_prefix, auth_skip)
           VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, 1) RETURNING *"#,
    )
    .bind(&route_id)
    .bind(format!("svc-{}", namespace))
    .bind(&path_prefix)
    .bind(&methods_json)
    .bind(&upstream_id)
    .bind(upstream_path_prefix)
    .fetch_one(&mut *tx)
    .await?;

    let description = body.description.unwrap_or_default();
    let tags = body.tags.unwrap_or_default();
    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    let status = body.status.unwrap_or_else(|| "stable".to_string());
    let soap_meta_str = soap_metadata.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

    let service_id = Uuid::new_v4().to_string();
    let service_row: SqliteService = sqlx::query_as(
        r#"INSERT INTO services (id, namespace, spec_url, spec_hash, upstream_id, route_id, description, tags, status, spec_content, service_type, soap_metadata)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) RETURNING *"#,
    )
    .bind(&service_id)
    .bind(&namespace)
    .bind(&spec_url)
    .bind(&spec_hash)
    .bind(&upstream_id)
    .bind(&route_id)
    .bind(&description)
    .bind(&tags_json)
    .bind(&status)
    .bind(&spec_text)
    .bind(service_type)
    .bind(&soap_meta_str)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE routes SET service_id = ?1, updated_at = datetime('now') WHERE id = ?2")
        .bind(&service_id)
        .bind(&route_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let service: shared::models::Service = service_row.into();
    Ok((
        axum::http::StatusCode::CREATED,
        Json(service_to_response(service)),
    ))
}

// Lightweight row type for list queries (excludes spec_content/soap_metadata)
#[derive(sqlx::FromRow)]
struct SqliteServiceListRow {
    id: String,
    namespace: String,
    version: i32,
    spec_url: String,
    spec_hash: String,
    upstream_id: String,
    route_id: Option<String>,
    description: String,
    tags: String,
    status: String,
    service_type: String,
    created_at: String,
    updated_at: String,
}

impl SqliteServiceListRow {
    fn to_response(self) -> ServiceResponse {
        let tags: Vec<String> = serde_json::from_str(&self.tags).unwrap_or_default();
        ServiceResponse {
            id: crate::models::parse_uuid(&self.id),
            namespace: self.namespace,
            version: self.version,
            spec_url: self.spec_url,
            spec_hash: self.spec_hash,
            upstream_id: crate::models::parse_uuid(&self.upstream_id),
            route_id: self.route_id.as_deref().map(crate::models::parse_uuid),
            description: self.description,
            tags,
            status: self.status,
            service_type: self.service_type,
            created_at: crate::models::parse_dt(&self.created_at),
            updated_at: crate::models::parse_dt(&self.updated_at),
        }
    }
}

pub async fn list_services(
    State(pool): State<SqlitePool>,
    Query(params): Query<ServiceFilterParams>,
) -> Result<Json<ListResponse<ServiceResponse>>, AppError> {
    let (page, limit, offset) = params.pagination.resolve();

    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx = 1u32;

    if params.search.is_some() {
        conditions.push(format!("namespace LIKE ?{param_idx} ESCAPE '\\'"));
        param_idx += 1;
    }
    if params.status.is_some() {
        conditions.push(format!("status = ?{param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM services {where_clause}");
    let list_sql = format!(
        "SELECT id, namespace, version, spec_url, spec_hash, upstream_id, route_id, description, tags, status, service_type, created_at, updated_at FROM services {where_clause} ORDER BY created_at DESC LIMIT ?{param_idx} OFFSET ?{}",
        param_idx + 1
    );

    let mut count_query = sqlx::query_as::<_, (i64,)>(&count_sql);
    if let Some(ref search) = params.search {
        let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        count_query = count_query.bind(format!("%{escaped}%"));
    }
    if let Some(ref status) = params.status {
        count_query = count_query.bind(status);
    }
    let total: (i64,) = count_query.fetch_one(&pool).await?;

    let mut list_query = sqlx::query_as::<_, SqliteServiceListRow>(&list_sql);
    if let Some(ref search) = params.search {
        let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        list_query = list_query.bind(format!("%{escaped}%"));
    }
    if let Some(ref status) = params.status {
        list_query = list_query.bind(status);
    }
    list_query = list_query.bind(limit).bind(offset);

    let rows: Vec<SqliteServiceListRow> = list_query.fetch_all(&pool).await?;

    Ok(Json(ListResponse {
        data: rows.into_iter().map(|r| r.to_response()).collect(),
        total: total.0,
        page,
        limit,
    }))
}

pub async fn get_service(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<Json<ServiceResponse>, AppError> {
    let row: SqliteServiceListRow =
        sqlx::query_as("SELECT id, namespace, version, spec_url, spec_hash, upstream_id, route_id, description, tags, status, service_type, created_at, updated_at FROM services WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    Ok(Json(row.to_response()))
}

pub async fn update_service(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateService>,
) -> Result<Json<ServiceResponse>, AppError> {
    let existing: (String, String, String) =
        sqlx::query_as("SELECT description, tags, status FROM services WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    let description = body.description.unwrap_or(existing.0);
    let tags = body.tags.unwrap_or_else(|| serde_json::from_str(&existing.1).unwrap_or_default());
    let status = body.status.unwrap_or(existing.2);

    if !matches!(status.as_str(), "alpha" | "beta" | "stable" | "deprecated") {
        return Err(AppError::Validation(
            "status must be 'alpha', 'beta', 'stable', or 'deprecated'".into(),
        ));
    }

    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());

    let updated_row: SqliteServiceListRow = sqlx::query_as(
        "UPDATE services SET description = ?1, tags = ?2, status = ?3, updated_at = datetime('now') WHERE id = ?4 RETURNING id, namespace, version, spec_url, spec_hash, upstream_id, route_id, description, tags, status, service_type, created_at, updated_at",
    )
    .bind(&description)
    .bind(&tags_json)
    .bind(&status)
    .bind(id.to_string())
    .fetch_one(&pool)
    .await?;

    Ok(Json(updated_row.to_response()))
}

pub async fn get_service_spec(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT spec_content FROM services WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?;

    let (spec_content,) = row
        .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    match spec_content {
        Some(content) => {
            let parsed: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| AppError::Internal(format!("Failed to parse stored spec: {}", e)))?;
            Ok(Json(parsed))
        }
        None => Ok(Json(serde_json::json!(null))),
    }
}

pub async fn delete_service(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let row: (String, Option<String>,) =
        sqlx::query_as("SELECT upstream_id, route_id FROM services WHERE id = ?1")
            .bind(id.to_string())
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Service not found".into()))?;

    let upstream_id = row.0;
    let route_id = row.1;

    // Use a transaction for multi-table delete
    let mut tx = pool.begin().await?;

    if let Some(ref route_id) = route_id {
        sqlx::query("DELETE FROM routes WHERE id = ?1")
            .bind(route_id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM upstreams WHERE id = ?1")
        .bind(&upstream_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM services WHERE id = ?1")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
