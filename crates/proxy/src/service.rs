use crate::lb::{self, Algorithm, ConnectionTracker};
use crate::logging::RequestLogEntry;
use crate::metrics;
use crate::router::GatewayConfig;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

/// Context carried through the request lifecycle.
pub struct GatewayCtx {
    pub request_start: Instant,
    pub route_id: Option<Uuid>,
    pub upstream_id: Option<Uuid>,
    pub upstream_target: Option<String>,
    /// The target ID selected by load balancing (for connection tracking).
    pub target_id: Option<Uuid>,
    pub strip_prefix: bool,
    pub path_prefix: String,
    /// The client identity for rate limiting (IP or API key hash).
    pub client_identity: Option<String>,
}

/// Simple sliding window rate limiter entry.
struct RateLimiterEntry {
    /// Timestamps of requests within the current window.
    timestamps: Vec<Instant>,
}

pub struct GatewayProxy {
    pub db_pool: Arc<PgPool>,
    pub config: Arc<ArcSwap<GatewayConfig>>,
    pub rr_counter: AtomicUsize,
    /// Rate limiter state: key = "{route_id}:{client_identity}", value = request timestamps.
    rate_limiters: Mutex<HashMap<String, RateLimiterEntry>>,
    /// Active connection tracker for least-connections algorithm (shared with health checker).
    pub conn_tracker: Arc<Mutex<ConnectionTracker>>,
    /// Async log sender for request logging to PostgreSQL.
    log_sender: tokio::sync::mpsc::UnboundedSender<RequestLogEntry>,
}

impl GatewayProxy {
    pub fn new(
        db_pool: Arc<PgPool>,
        config: Arc<ArcSwap<GatewayConfig>>,
        conn_tracker: Arc<Mutex<ConnectionTracker>>,
        log_sender: tokio::sync::mpsc::UnboundedSender<RequestLogEntry>,
    ) -> Self {
        Self {
            db_pool,
            config,
            rr_counter: AtomicUsize::new(0),
            rate_limiters: Mutex::new(HashMap::new()),
            conn_tracker,
            log_sender,
        }
    }

    /// Check rate limit for a given route and client identity.
    /// Returns Ok(remaining) or Err(retry_after_secs).
    fn check_rate_limit(
        &self,
        route_id: &Uuid,
        client_identity: &str,
        requests_per_second: i32,
    ) -> Result<i32, u64> {
        let key = format!("{}:{}", route_id, client_identity);
        let now = Instant::now();
        let window = std::time::Duration::from_secs(1);

        let mut limiters = self.rate_limiters.lock().unwrap();
        let entry = limiters.entry(key).or_insert_with(|| RateLimiterEntry {
            timestamps: Vec::new(),
        });

        // Remove timestamps outside the 1-second window
        entry.timestamps.retain(|ts| now.duration_since(*ts) < window);

        if entry.timestamps.len() >= requests_per_second as usize {
            // Rate limited
            let oldest = entry.timestamps.first().unwrap();
            let retry_after = window
                .checked_sub(now.duration_since(*oldest))
                .unwrap_or(window);
            Err(retry_after.as_secs().max(1))
        } else {
            entry.timestamps.push(now);
            let remaining = requests_per_second as i32 - entry.timestamps.len() as i32;
            Ok(remaining)
        }
    }
}

#[async_trait]
impl ProxyHttp for GatewayProxy {
    type CTX = GatewayCtx;

    fn new_ctx(&self) -> Self::CTX {
        GatewayCtx {
            request_start: Instant::now(),
            route_id: None,
            upstream_id: None,
            upstream_target: None,
            target_id: None,
            strip_prefix: false,
            path_prefix: String::new(),
            client_identity: None,
        }
    }

    async fn request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<bool> {
        let method = session.req_header().method.as_str();
        let path = session.req_header().uri.path().to_string();
        tracing::debug!(method = %method, path = %path, "Incoming request");

        let config = self.config.load();

        // --- 1. Route matching ---
        let route = match config.match_route(&path, method) {
            Some(r) => r,
            None => {
                let _ = session.respond_error(404).await;
                return Ok(true);
            }
        };

        let route_id = route.id;
        ctx.route_id = Some(route_id);
        ctx.upstream_id = Some(route.upstream_id);
        ctx.strip_prefix = route.strip_prefix;
        ctx.path_prefix = route.path_prefix.clone();

        // --- 2. API Key Authentication ---
        if config.route_requires_auth(&route_id) {
            let key_header = session
                .req_header()
                .headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            match key_header {
                None => {
                    metrics::AUTH_FAILURES.with_label_values(&[&route_id.to_string()]).inc();
                    return self
                        .send_json_error(session, 401, "API key required", "AUTH_REQUIRED")
                        .await;
                }
                Some(plaintext_key) => {
                    // Hash the provided key
                    let mut hasher = Sha256::new();
                    hasher.update(plaintext_key.as_bytes());
                    let key_hash = hex::encode(hasher.finalize());

                    match config.validate_api_key(&route_id, &key_hash) {
                        Ok(identity) => {
                            ctx.client_identity = Some(identity);
                        }
                        Err(msg) => {
                            metrics::AUTH_FAILURES.with_label_values(&[&route_id.to_string()]).inc();
                            return self
                                .send_json_error(session, 401, msg, "AUTH_FAILED")
                                .await;
                        }
                    }
                }
            }
        }

        // --- 3. Rate Limiting ---
        if let Some(rate_limit) = config.get_rate_limit(&route_id) {
            // Determine client identity
            let client_id = match rate_limit.limit_by.as_str() {
                "api_key" => ctx
                    .client_identity
                    .clone()
                    .unwrap_or_else(|| self.get_client_ip(session)),
                _ => self.get_client_ip(session),
            };

            match self.check_rate_limit(
                &route_id,
                &client_id,
                rate_limit.requests_per_second,
            ) {
                Ok(remaining) => {
                    // Add rate limit headers (will be set in response_filter)
                    ctx.client_identity
                        .get_or_insert(client_id);
                    // Store remaining for response headers — we'll use logging for now
                    let _ = remaining;
                }
                Err(retry_after) => {
                    metrics::RATE_LIMIT_HITS
                        .with_label_values(&[&route_id.to_string(), &rate_limit.limit_by])
                        .inc();
                    return self
                        .send_rate_limit_error(
                            session,
                            retry_after,
                            rate_limit.requests_per_second,
                        )
                        .await;
                }
            }
        }

        Ok(false)
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let upstream_id = ctx
            .upstream_id
            .as_ref()
            .ok_or_else(|| pingora::Error::new_str("No upstream selected"))?;

        let config = self.config.load();
        let healthy = config.healthy_targets(upstream_id);

        if healthy.is_empty() {
            // Return 503 with JSON body
            let _ = self
                .send_json_error(
                    session,
                    503,
                    "All upstream targets are unhealthy",
                    "UPSTREAM_UNAVAILABLE",
                )
                .await;
            return Err(pingora::Error::new_str(
                "All upstream targets are unhealthy",
            ));
        }

        // Determine algorithm from upstream config
        let algorithm = config
            .upstreams
            .get(upstream_id)
            .map(|u| Algorithm::from_str(&u.algorithm))
            .unwrap_or(Algorithm::RoundRobin);

        let target = match algorithm {
            Algorithm::RoundRobin => lb::select_round_robin(&healthy, &self.rr_counter),
            Algorithm::WeightedRoundRobin => {
                lb::select_weighted_round_robin(&healthy, &self.rr_counter)
            }
            Algorithm::LeastConnections => {
                let tracker = self.conn_tracker.lock().unwrap();
                lb::select_least_connections(&healthy, &tracker)
            }
        };

        let target = target.ok_or_else(|| pingora::Error::new_str("No target selected"))?;

        ctx.upstream_target = Some(format!("{}:{}", target.host, target.port));
        ctx.target_id = Some(target.id);

        // Increment connection count for least-connections tracking
        {
            let tracker = self.conn_tracker.lock().unwrap();
            tracker.increment(&target.id);
        }
        metrics::ACTIVE_CONNECTIONS.inc();

        let peer = HttpPeer::new(
            (target.host.as_str(), target.port as u16),
            false,
            target.host.clone(),
        );
        Ok(Box::new(peer))
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut pingora::http::RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Set Host header
        if let Some(ref target) = ctx.upstream_target {
            let host = target.split(':').next().unwrap_or(target);
            upstream_request.insert_header("Host", host).unwrap();
        }

        // Strip path prefix
        if ctx.strip_prefix && !ctx.path_prefix.is_empty() {
            let original_path = session.req_header().uri.path().to_string();
            let new_path = if original_path.len() > ctx.path_prefix.len() {
                &original_path[ctx.path_prefix.len()..]
            } else {
                "/"
            };
            let new_path = if new_path.starts_with('/') {
                new_path.to_string()
            } else {
                format!("/{new_path}")
            };
            let query = session.req_header().uri.query();
            let new_uri = if let Some(q) = query {
                format!("{new_path}?{q}")
            } else {
                new_path
            };
            upstream_request.set_uri(new_uri.parse().unwrap());
        }

        // Security: set forwarded headers
        let client_ip = self.get_client_ip(session);
        upstream_request.insert_header("X-Forwarded-For", &client_ip).unwrap();
        upstream_request.insert_header("X-Forwarded-Proto", "http").unwrap();
        if let Some(host_header) = session.req_header().headers.get("host") {
            upstream_request
                .insert_header("X-Forwarded-Host", host_header.to_str().unwrap_or(""))
                .unwrap();
        }

        // Security: remove internal headers from proxied requests
        upstream_request.remove_header("x-admin-token");

        // Gateway headers
        upstream_request
            .insert_header(
                "X-Gateway-Route",
                ctx.route_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .unwrap();
        upstream_request
            .insert_header("X-Gateway-Request-Id", &Uuid::new_v4().to_string())
            .unwrap();

        Ok(())
    }

    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) {
        // Decrement connection count for least-connections tracking
        if let Some(target_id) = ctx.target_id {
            let tracker = self.conn_tracker.lock().unwrap();
            tracker.decrement(&target_id);
            metrics::ACTIVE_CONNECTIONS.dec();
        }

        let duration = ctx.request_start.elapsed();
        let status = session
            .response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);
        let method = session.req_header().method.as_str();
        let path = session.req_header().uri.path();
        let route_label = ctx
            .route_id
            .map(|id| id.to_string())
            .unwrap_or_default();

        // Record Prometheus metrics
        metrics::REQUESTS_TOTAL
            .with_label_values(&[&route_label, method, &status.to_string()])
            .inc();
        metrics::REQUEST_DURATION
            .with_label_values(&[&route_label, method])
            .observe(duration.as_secs_f64());

        tracing::info!(
            method = %method,
            path = %path,
            status = status,
            latency_ms = duration.as_secs_f64() * 1000.0,
            upstream = ctx.upstream_target.as_deref().unwrap_or("-"),
            route_id = route_label,
            "Request completed"
        );

        // Send log entry to async writer
        let _ = self.log_sender.send(RequestLogEntry {
            route_id: ctx.route_id,
            method: method.to_string(),
            path: path.to_string(),
            status_code: status as i32,
            latency_ms: duration.as_secs_f64() * 1000.0,
            client_ip: self.get_client_ip(session),
            upstream_target: ctx.upstream_target.clone(),
        });
    }
}

// Helper methods
impl GatewayProxy {
    fn get_client_ip(&self, session: &Session) -> String {
        // Try X-Forwarded-For first
        if let Some(xff) = session.req_header().headers.get("x-forwarded-for") {
            if let Ok(s) = xff.to_str() {
                if let Some(first_ip) = s.split(',').next() {
                    return first_ip.trim().to_string();
                }
            }
        }

        // Fallback to connection peer address (strip port to get just IP)
        session
            .client_addr()
            .map(|a| {
                let addr = a.to_string();
                // Strip port from "ip:port" format
                addr.rsplit_once(':')
                    .map(|(ip, _port)| ip.to_string())
                    .unwrap_or(addr)
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    async fn send_json_error(
        &self,
        session: &mut Session,
        status: u16,
        message: &str,
        code: &str,
    ) -> Result<bool> {
        let body = serde_json::json!({
            "error": message,
            "code": code,
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();

        let mut header = pingora::http::ResponseHeader::build(status, None).unwrap();
        header
            .insert_header("Content-Type", "application/json")
            .unwrap();
        header
            .insert_header("Content-Length", &body_bytes.len().to_string())
            .unwrap();

        session.write_response_header(Box::new(header), false).await?;
        session
            .write_response_body(Some(bytes::Bytes::from(body_bytes)), true)
            .await?;

        Ok(true)
    }

    async fn send_rate_limit_error(
        &self,
        session: &mut Session,
        retry_after: u64,
        limit: i32,
    ) -> Result<bool> {
        let body = serde_json::json!({
            "error": "Rate limit exceeded",
            "code": "RATE_LIMITED",
        });
        let body_bytes = serde_json::to_vec(&body).unwrap();

        let mut header = pingora::http::ResponseHeader::build(429, None).unwrap();
        header
            .insert_header("Content-Type", "application/json")
            .unwrap();
        header
            .insert_header("Content-Length", &body_bytes.len().to_string())
            .unwrap();
        header
            .insert_header("Retry-After", &retry_after.to_string())
            .unwrap();
        header
            .insert_header("X-RateLimit-Limit", &limit.to_string())
            .unwrap();
        header
            .insert_header("X-RateLimit-Remaining", "0")
            .unwrap();

        session.write_response_header(Box::new(header), false).await?;
        session
            .write_response_body(Some(bytes::Bytes::from(body_bytes)), true)
            .await?;

        Ok(true)
    }
}
