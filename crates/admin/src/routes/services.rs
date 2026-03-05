use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::net::IpAddr;
use uuid::Uuid;

use crate::errors::AppError;
use crate::AppSettings;

/// Check if an IP address is private/reserved (SSRF protection).
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()          // 127.0.0.0/8
                || v4.is_private()     // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local()  // 169.254/16
                || v4.is_broadcast()   // 255.255.255.255
                || v4.is_unspecified() // 0.0.0.0
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64 // 100.64/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified()
        }
    }
}

/// Validate a URL for SSRF: resolve DNS and reject private IPs.
async fn validate_url_ssrf(url_str: &str) -> Result<(), AppError> {
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

    // Try to parse as IP directly
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            return Err(AppError::Validation(
                "URL resolves to a private/reserved IP address".into(),
            ));
        }
    } else {
        // DNS resolve and check all addresses
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
    }

    Ok(())
}

// --- DTOs ---

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
/// `source_url` is used as a base when the spec's server URL is relative.
/// `override_url` is a user-provided fallback when the spec has no server info.
fn parse_spec_server(
    spec: &serde_json::Value,
    source_url: &str,
    override_url: Option<&str>,
) -> Result<(String, String, u16, bool), AppError> {
    // Try OpenAPI 3.x `servers[0].url` first, then fall back to Swagger 2.0 `host`/`basePath`/`schemes`
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
        // Swagger 2.0: construct URL from host, basePath, and schemes
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

    // Try parsing as absolute URL first; fall back to resolving against source_url
    let parsed = match url::Url::parse(server_url) {
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            // Resolve the relative path against the spec's source URL
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
    Extension(settings): Extension<AppSettings>,
    Json(body): Json<ImportRequest>,
) -> Result<(axum::http::StatusCode, Json<ServiceResponse>), AppError> {
    let max_spec_bytes = settings.max_spec_size_bytes;
    // Input length validation
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
            // SSRF protection: block private/reserved IP addresses
            validate_url_ssrf(url).await?;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(5))
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

            // Enforce response size limit
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

    // Compute hash
    let mut hasher = Sha256::new();
    hasher.update(&spec_bytes);
    let spec_hash = hex::encode(hasher.finalize());

    // Parse JSON
    let spec: serde_json::Value = serde_json::from_slice(&spec_bytes)
        .map_err(|e| AppError::Validation(format!("Invalid JSON in spec: {}", e)))?;

    // Validate that spec has at least one endpoint
    let has_paths = spec
        .get("paths")
        .and_then(|p| p.as_object())
        .is_some_and(|p| !p.is_empty());
    if !has_paths {
        return Err(AppError::Validation(
            "Spec has no endpoints — paths object is empty or missing".into(),
        ));
    }

    // Extract server info
    let (base_path, host, port, tls) =
        parse_spec_server(&spec, &spec_url, body.server_url.as_deref())?;

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

        // Update service record (including spec_content)
        let spec_text = String::from_utf8_lossy(&spec_bytes).to_string();
        let updated: shared::models::Service = sqlx::query_as(
            "UPDATE services SET version = $1, spec_url = $2, spec_hash = $3, spec_content = $4, updated_at = now() WHERE id = $5 RETURNING *",
        )
        .bind(new_version)
        .bind(&spec_url)
        .bind(&spec_hash)
        .bind(&spec_text)
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
        r#"INSERT INTO routes (name, path_prefix, upstream_id, strip_prefix, upstream_path_prefix, auth_skip)
           VALUES ($1, $2, $3, true, $4, true) RETURNING *"#,
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

    let spec_text = String::from_utf8_lossy(&spec_bytes).to_string();
    let service: shared::models::Service = sqlx::query_as(
        r#"INSERT INTO services (namespace, spec_url, spec_hash, upstream_id, route_id, description, tags, status, spec_content)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *"#,
    )
    .bind(&namespace)
    .bind(&spec_url)
    .bind(&spec_hash)
    .bind(upstream.id)
    .bind(route.id)
    .bind(&description)
    .bind(&tags)
    .bind(&status)
    .bind(&spec_text)
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
        // Escape ILIKE wildcards to prevent injection of % and _
        let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        count_query = count_query.bind(format!("%{escaped}%"));
    }
    if let Some(ref status) = params.status {
        count_query = count_query.bind(status);
    }
    let total: (i64,) = count_query.fetch_one(&pool).await?;

    // Build list query
    let mut list_query = sqlx::query_as::<_, shared::models::Service>(&list_sql);
    if let Some(ref search) = params.search {
        let escaped = search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        list_query = list_query.bind(format!("%{escaped}%"));
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

pub async fn get_service_spec(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT spec_content FROM services WHERE id = $1")
            .bind(id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // --- is_private_ip ---

    #[test]
    fn private_ip_loopback_v4() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 255, 255, 255))));
    }

    #[test]
    fn private_ip_rfc1918_10() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 255, 255, 255))));
    }

    #[test]
    fn private_ip_rfc1918_172() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255))));
        // 172.32.x.x is NOT private
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
    }

    #[test]
    fn private_ip_rfc1918_192() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 255, 255))));
    }

    #[test]
    fn private_ip_link_local() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn private_ip_broadcast() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255))));
    }

    #[test]
    fn private_ip_unspecified() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
    }

    #[test]
    fn private_ip_cgnat() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 127, 255, 255))));
        // 100.128.x.x is NOT CGNAT
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));
    }

    #[test]
    fn private_ip_v6_loopback() {
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn public_ip_allowed() {
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1))));
    }

    // --- validate_url_ssrf ---

    #[tokio::test]
    async fn ssrf_blocks_loopback_ip() {
        let result = validate_url_ssrf("http://127.0.0.1/spec.json").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::errors::AppError::Validation(msg) => assert!(msg.contains("private"), "expected 'private' in: {msg}"),
            other => panic!("expected Validation error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn ssrf_blocks_private_ip() {
        let result = validate_url_ssrf("http://192.168.1.1/spec.json").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ssrf_blocks_10_network() {
        let result = validate_url_ssrf("http://10.0.0.1/spec.json").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ssrf_blocks_unspecified() {
        let result = validate_url_ssrf("http://0.0.0.0/spec.json").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ssrf_blocks_ftp_scheme() {
        let result = validate_url_ssrf("ftp://example.com/spec.json").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            crate::errors::AppError::Validation(msg) => assert!(msg.contains("http"), "expected 'http' in: {msg}"),
            other => panic!("expected Validation error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn ssrf_blocks_file_scheme() {
        let result = validate_url_ssrf("file:///etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ssrf_rejects_invalid_url() {
        let result = validate_url_ssrf("not-a-url").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ssrf_allows_public_ip() {
        // 8.8.8.8 is a public IP — should be allowed
        let result = validate_url_ssrf("https://8.8.8.8/spec.json").await;
        assert!(result.is_ok());
    }

    // --- slugify ---

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Pet Store"), "pet-store");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("My API v2.0!"), "my-api-v2-0");
    }

    #[test]
    fn slugify_already_slug() {
        assert_eq!(slugify("my-api"), "my-api");
    }

    #[test]
    fn slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_only_special() {
        assert_eq!(slugify("!!!"), "");
    }
}
