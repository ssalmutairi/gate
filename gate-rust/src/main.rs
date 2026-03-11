use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Deserialize)]
struct ServiceEntry {
    name: String,
    ip: String,
    port: u16,
    #[serde(rename = "api-key")]
    api_key: Option<String>,
    tls: Option<bool>,
    timeout: Option<u64>,
    host: Option<String>,
}

impl ServiceEntry {
    fn base_url(&self) -> String {
        let scheme = if self.tls.unwrap_or(false) { "https" } else { "http" };
        format!("{scheme}://{}:{}", self.ip, self.port)
    }

    fn timeout_secs(&self) -> u64 {
        self.timeout.filter(|&t| t > 0).unwrap_or(30)
    }
}

#[derive(Clone)]
struct AppState {
    services: Arc<HashMap<String, ServiceEntry>>,
    client: Client,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "gate_rust=info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    let proxy_json = std::env::var("PROXY").unwrap_or_else(|_| {
        warn!("PROXY env var is not set — no services configured");
        "[]".to_string()
    });

    let entries: Vec<ServiceEntry> =
        serde_json::from_str(&proxy_json).expect("Failed to parse PROXY env var");

    let mut services = HashMap::new();
    for entry in entries {
        info!(
            "Service registered: {} -> {}:{} (api-key: {})",
            entry.name,
            entry.ip,
            entry.port,
            if entry.api_key.is_some() { "***" } else { "none" }
        );
        services.insert(entry.name.clone(), entry);
    }
    info!("Loaded {} services from PROXY", services.len());

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client");

    let state = AppState {
        services: Arc::new(services),
        client,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/services", get(list_services))
        .fallback(proxy_handler)
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("Failed to bind");

    info!("gate-rust listening on :{port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "version": "1.0.0"}))
}

async fn list_services(State(state): State<AppState>) -> Json<serde_json::Value> {
    let services: Vec<serde_json::Value> = state
        .services
        .values()
        .map(|s| {
            let mut m = serde_json::Map::new();
            m.insert("name".into(), s.name.clone().into());
            m.insert("url".into(), s.base_url().into());
            m.insert("timeout".into(), s.timeout_secs().into());
            m.insert("auth".into(), s.api_key.is_some().into());
            if let Some(host) = &s.host {
                m.insert("host".into(), host.clone().into());
            }
            serde_json::Value::Object(m)
        })
        .collect();

    let total = services.len();
    Json(serde_json::json!({"services": services, "total": total}))
}

async fn proxy_handler(State(state): State<AppState>, req: Request) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string());

    // Parse: /{service_name}/{remaining}
    let trimmed = path.trim_start_matches('/');
    let (service_name, remaining) = match trimmed.find('/') {
        Some(i) => (&trimmed[..i], &trimmed[i..]),
        None => (trimmed, "/"),
    };

    let Some(service) = state.services.get(service_name) else {
        warn!("{method} {path} -> 404 (unknown service: {service_name})");
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorBody { error: format!("Service not found: {service_name}") }),
        )
            .into_response();
    };

    // API key validation (constant-time comparison)
    if let Some(expected_key) = &service.api_key {
        let valid = req
            .headers()
            .get("X-API-KEY")
            .and_then(|v| v.to_str().ok())
            .map(|k| constant_time_eq(k.as_bytes(), expected_key.as_bytes()))
            .unwrap_or(false);

        if !valid {
            warn!("{method} {path} -> 401 (invalid or missing X-API-KEY for service: {service_name})");
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody { error: "Unauthorized: invalid or missing X-API-KEY".into() }),
            )
                .into_response();
        }
    }

    let target_url = match &query {
        Some(q) => format!("{}{remaining}?{q}", service.base_url()),
        None => format!("{}{remaining}", service.base_url()),
    };

    // Build upstream request headers
    let mut headers = HeaderMap::new();
    for (name, value) in req.headers() {
        match name.as_str() {
            "host" | "accept-encoding" | "connection" | "transfer-encoding" | "x-api-key" => {}
            _ => { headers.insert(name.clone(), value.clone()); }
        }
    }
    if let Some(host) = &service.host {
        if let Ok(v) = HeaderValue::from_str(host) {
            headers.insert("host", v);
        }
    }

    // Read request body
    let body_bytes = match axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            error!("{method} {path} -> 413 body too large: {e}");
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ErrorBody { error: "Request body too large".into() }),
            )
                .into_response();
        }
    };

    // Forward request
    let mut upstream = state
        .client
        .request(method.clone(), &target_url)
        .headers(headers)
        .timeout(Duration::from_secs(service.timeout_secs()));

    if !body_bytes.is_empty() {
        upstream = upstream.body(body_bytes);
    }

    match upstream.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            info!("{method} {path} -> {status} {ms:.1}ms [{target_url}]");

            let mut response_headers = HeaderMap::new();
            for (name, value) in resp.headers() {
                match name.as_str() {
                    "transfer-encoding" | "content-length" => {}
                    _ => { response_headers.insert(name.clone(), value.clone()); }
                }
            }

            let body = resp.bytes().await.unwrap_or_default();
            let mut response = (status, Body::from(body)).into_response();
            *response.headers_mut() = response_headers;
            response
        }
        Err(e) => {
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            if e.is_timeout() {
                error!("{method} {path} -> 504 {ms:.1}ms [{target_url}] timeout after {}s", service.timeout_secs());
                (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(ErrorBody { error: format!("Request timed out after {}s", service.timeout_secs()) }),
                )
                    .into_response()
            } else {
                error!("{method} {path} -> 502 {ms:.1}ms [{target_url}] error: {e}");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorBody { error: format!("Upstream unreachable: {service_name}") }),
                )
                    .into_response()
            }
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    info!("Shutting down...");
}
