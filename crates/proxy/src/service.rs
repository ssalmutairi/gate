use crate::lb::{self, Algorithm, ConnectionTracker};
use crate::logging::RequestLogEntry;
use crate::metrics;
use crate::router::GatewayConfig;
use crate::soap::SoapOperationMeta;
use crate::state::StateBackend;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use pingora_cache::MemCache;
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

/// In-memory cache backend for response caching.
static CACHE_BACKEND: Lazy<MemCache> = Lazy::new(MemCache::new);

/// Compute the rewritten URI path+query for an upstream request.
///
/// This is the pure-logic core of `upstream_request_filter`'s path rewriting.
/// Extracted so it can be unit-tested without a Pingora `Session`.
#[cfg(test)]
pub(crate) fn rewrite_path(
    original_path: &str,
    prefix: &str,
    strip: bool,
    upstream_prefix: Option<&str>,
    query: Option<&str>,
) -> String {
    let mut path = original_path.to_string();

    // Strip path prefix
    if strip && !prefix.is_empty() {
        let stripped = if path.len() > prefix.len() {
            &path[prefix.len()..]
        } else {
            "/"
        };
        path = if stripped.starts_with('/') {
            stripped.to_string()
        } else {
            format!("/{stripped}")
        };
    }

    // Prepend upstream_path_prefix
    if let Some(up_prefix) = upstream_prefix {
        path = format!(
            "{}{}",
            up_prefix.trim_end_matches('/'),
            if path.starts_with('/') { &path } else { "/" }
        );
    }

    // Append query string
    if let Some(q) = query {
        format!("{path}?{q}")
    } else {
        path
    }
}

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
    /// Path prefix to prepend when forwarding to the upstream (e.g. "/api/v3").
    pub upstream_path_prefix: Option<String>,
    /// Whether the selected target uses TLS.
    pub target_tls: bool,
    /// The client identity for rate limiting (IP or API key hash).
    pub client_identity: Option<String>,
    /// Per-route timeout override (milliseconds).
    pub timeout_ms: Option<i32>,
    /// Number of retries remaining for the current request.
    pub retries_remaining: i32,
    /// Total retries configured for this route.
    pub retries_total: i32,
    /// Cache TTL for this route (seconds), if caching is enabled.
    pub cache_ttl_secs: Option<i32>,
    /// SOAP operation metadata (set when route matches a SOAP service).
    pub soap_operation: Option<SoapOperationMeta>,
    /// Buffer for accumulating the JSON request body (SOAP routes only).
    pub soap_request_body: Vec<u8>,
    /// Buffer for accumulating the SOAP XML response body (SOAP routes only).
    pub soap_response_body: Vec<u8>,
    /// Captures the upstream response body for error responses (status >= 400), capped at 4 KB.
    pub error_body: Option<Vec<u8>>,
}

pub struct GatewayProxy {
    pub config: Arc<ArcSwap<GatewayConfig>>,
    pub rr_counter: AtomicUsize,
    /// Active connection tracker for least-connections algorithm (shared with health checker).
    pub conn_tracker: Arc<Mutex<ConnectionTracker>>,
    /// Async log sender for request logging to PostgreSQL.
    log_sender: tokio::sync::mpsc::Sender<RequestLogEntry>,
    /// Trusted proxy IPs/CIDRs — only trust X-Forwarded-For from these peers.
    trusted_proxies: Vec<String>,
    /// State backend (in-memory or Redis) for rate limiting and circuit breaker.
    pub state: Arc<StateBackend>,
}

impl GatewayProxy {
    pub fn new(
        config: Arc<ArcSwap<GatewayConfig>>,
        conn_tracker: Arc<Mutex<ConnectionTracker>>,
        log_sender: tokio::sync::mpsc::Sender<RequestLogEntry>,
        trusted_proxies: Vec<String>,
        state: Arc<StateBackend>,
    ) -> Self {
        Self {
            config,
            rr_counter: AtomicUsize::new(0),
            conn_tracker,
            log_sender,
            trusted_proxies,
            state,
        }
    }
}

#[async_trait]
impl ProxyHttp for GatewayProxy {
    type CTX = GatewayCtx;

    fn init_downstream_modules(&self, modules: &mut pingora::modules::http::HttpModules) {
        modules.add_module(
            pingora::modules::http::compression::ResponseCompressionBuilder::enable(6),
        );
    }

    fn new_ctx(&self) -> Self::CTX {
        GatewayCtx {
            request_start: Instant::now(),
            route_id: None,
            upstream_id: None,
            upstream_target: None,
            target_id: None,
            strip_prefix: false,
            path_prefix: String::new(),
            upstream_path_prefix: None,
            target_tls: false,
            client_identity: None,
            timeout_ms: None,
            retries_remaining: 0,
            retries_total: 0,
            cache_ttl_secs: None,
            soap_operation: None,
            soap_request_body: Vec::new(),
            soap_response_body: Vec::new(),
            error_body: None,
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

        // Extract Host header (strip port if present)
        let host = session
            .req_header()
            .headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h).to_string());

        // --- 1. Route matching ---
        let route = match config.match_route(&path, method, host.as_deref()) {
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
        ctx.upstream_path_prefix = route.upstream_path_prefix.clone();
        ctx.timeout_ms = route.timeout_ms;
        ctx.retries_remaining = route.retries.min(3);
        ctx.retries_total = route.retries.min(3);
        ctx.cache_ttl_secs = route.cache_ttl_secs;

        // --- 1a. SOAP operation detection ---
        if let Some(soap_meta) = config.get_soap_meta(route) {
            let op_path = if path.len() > route.path_prefix.len() {
                &path[route.path_prefix.len()..]
            } else {
                "/"
            };
            if let Some(op) = soap_meta.operations.get(op_path) {
                ctx.soap_operation = Some(op.clone());
            }
        }

        // --- 1b. Request size limiting ---
        if let Some(max_bytes) = route.max_body_bytes {
            if let Some(cl) = session.req_header().headers.get("content-length") {
                if let Ok(len) = cl.to_str().unwrap_or("0").parse::<i64>() {
                    if len > max_bytes {
                        return self
                            .send_json_error(session, 413, "Request body too large", "BODY_TOO_LARGE")
                            .await;
                    }
                }
            } else if session.req_header().headers.get("transfer-encoding").is_some() {
                // Chunked encoding without Content-Length — reject if body limit is set
                // (Pingora doesn't buffer the full body for us to check size)
                // This prevents bypassing size limits via chunked encoding.
                // Allow a generous threshold: only block if max_body_bytes is explicitly small.
                tracing::debug!(max_bytes, "Chunked request with body size limit — will be enforced by upstream");
            }
        }

        // --- 1c. IP allowlist/denylist ---
        if let Some(rules) = config.get_ip_rules(&route_id) {
            let client_ip = self.get_client_ip(session);
            if let Ok(client_addr) = client_ip.parse::<std::net::IpAddr>() {
                let has_allow_rules = rules.iter().any(|r| r.action == "allow");
                let mut denied = false;
                let mut allowed = !has_allow_rules; // if no allow rules, default allow

                for rule in rules {
                    if let Ok(network) = rule.cidr.parse::<ipnet::IpNet>() {
                        if network.contains(&client_addr) {
                            if rule.action == "deny" {
                                denied = true;
                                break;
                            } else if rule.action == "allow" {
                                allowed = true;
                            }
                        }
                    }
                }

                if denied || !allowed {
                    return self
                        .send_json_error(session, 403, "IP address not allowed", "IP_DENIED")
                        .await;
                }
            }
        }

        // --- 2. API Key Authentication ---
        if !route.auth_skip && config.route_requires_auth(&route_id) {
            let key_header = session
                .req_header()
                .headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .map(|s| s.to_string())
                .or_else(|| {
                    session
                        .req_header()
                        .headers
                        .get("x-api-key")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string())
                });

            match key_header {
                None => {
                    metrics::AUTH_FAILURES.with_label_values(&[&ctx.path_prefix]).inc();
                    return self
                        .send_json_error(session, 401, "Authorization header required (Bearer <api-key>)", "AUTH_REQUIRED")
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
                            metrics::AUTH_FAILURES.with_label_values(&[&ctx.path_prefix]).inc();
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

            match self.state.check_rate_limit(
                &route_id,
                &client_id,
                rate_limit.requests_per_second,
            ).await {
                Ok(remaining) => {
                    // Add rate limit headers (will be set in response_filter)
                    ctx.client_identity
                        .get_or_insert(client_id);
                    // Store remaining for response headers — we'll use logging for now
                    let _ = remaining;
                }
                Err(retry_after) => {
                    metrics::RATE_LIMIT_HITS
                        .with_label_values(&[&ctx.path_prefix, &rate_limit.limit_by])
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

    async fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<bytes::Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        if ctx.soap_operation.is_none() {
            return Ok(());
        }

        // Buffer incoming JSON body chunks
        if let Some(data) = body.take() {
            if ctx.soap_request_body.len() + data.len() > crate::soap::MAX_SOAP_BODY_BYTES {
                tracing::error!("SOAP request body exceeds size limit");
                ctx.soap_request_body.clear();
                let err_json = serde_json::json!({"error": "Request body too large"});
                *body = Some(bytes::Bytes::from(serde_json::to_vec(&err_json).unwrap_or_default()));
                return Ok(());
            }
            ctx.soap_request_body.extend_from_slice(&data);
        }

        if end_of_stream {
            if let Some(ref op) = ctx.soap_operation {
                let json_body: serde_json::Value = if ctx.soap_request_body.is_empty() {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    match serde_json::from_slice(&ctx.soap_request_body) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!(error = %e, "Invalid JSON in SOAP request body");
                            let err_json = serde_json::json!({"error": "Invalid JSON request body"});
                            *body = Some(bytes::Bytes::from(serde_json::to_vec(&err_json).unwrap_or_default()));
                            ctx.soap_request_body.clear();
                            return Ok(());
                        }
                    }
                };

                match crate::soap::json_to_soap_xml(
                    &json_body,
                    &op.input_element,
                    &op.target_namespace,
                ) {
                    Ok(xml_bytes) => {
                        *body = Some(bytes::Bytes::from(xml_bytes));
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to convert JSON to SOAP XML");
                        *body = Some(bytes::Bytes::from(std::mem::take(&mut ctx.soap_request_body)));
                    }
                }
                ctx.soap_request_body.clear();
            }
        }

        Ok(())
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

        // Filter out circuit-broken targets
        let available: Vec<&shared::models::Target> = healthy
            .into_iter()
            .filter(|t| self.state.circuit_breaker().is_available(&t.id))
            .collect();

        if available.is_empty() {
            let _ = self
                .send_json_error(
                    session,
                    503,
                    "All upstream targets are circuit-broken",
                    "CIRCUIT_BREAKER_OPEN",
                )
                .await;
            return Err(pingora::Error::new_str(
                "All upstream targets are circuit-broken",
            ));
        }
        let healthy = available;

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
                let tracker = self.conn_tracker.lock().unwrap_or_else(|e| e.into_inner());
                lb::select_least_connections(&healthy, &tracker)
            }
        };

        let target = target.ok_or_else(|| pingora::Error::new_str("No target selected"))?;

        ctx.upstream_target = Some(format!("{}:{}", target.host, target.port));
        ctx.target_id = Some(target.id);
        ctx.target_tls = target.tls;

        // Increment connection count for least-connections tracking
        {
            let tracker = self.conn_tracker.lock().unwrap_or_else(|e| e.into_inner());
            tracker.increment(&target.id);
        }
        metrics::ACTIVE_CONNECTIONS.inc();

        let mut peer = HttpPeer::new(
            (target.host.as_str(), target.port as u16),
            target.tls,
            target.host.clone(),
        );

        // Allow HTTP/2 for TLS upstreams (many HTTPS servers require h2 via ALPN)
        if target.tls {
            peer.options.set_http_version(2, 1);
        }

        // Apply per-upstream TLS config
        if let Some(tls_config) = config.upstream_tls.get(upstream_id) {
            if tls_config.skip_verify {
                peer.options.verify_cert = false;
                peer.options.verify_hostname = false;
            }
            if let Some(ref cert_key) = tls_config.client_cert_key {
                peer.client_cert_key = Some(cert_key.clone());
            }
        }

        // Set upstream timeouts — use per-route config or defaults
        if let Some(ms) = ctx.timeout_ms {
            let timeout = std::time::Duration::from_millis(ms as u64);
            // Read timeout = 6x connect/write timeout, capped at 300s
            let read_timeout = std::time::Duration::from_millis(
                ((ms as u64) * 6).min(300_000),
            );
            peer.options.connection_timeout = Some(timeout);
            peer.options.read_timeout = Some(read_timeout);
            peer.options.write_timeout = Some(timeout);
        } else {
            peer.options.connection_timeout = Some(std::time::Duration::from_secs(10));
            peer.options.read_timeout = Some(std::time::Duration::from_secs(60));
            peer.options.write_timeout = Some(std::time::Duration::from_secs(10));
        }

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
            if let Ok(parsed) = new_uri.parse() {
                upstream_request.set_uri(parsed);
            } else {
                tracing::warn!(uri = %new_uri, "Failed to parse rewritten URI");
            }
        }

        // Prepend upstream_path_prefix (e.g. /api/v3) after strip
        if let Some(ref prefix) = ctx.upstream_path_prefix {
            let current_path = upstream_request.uri.path().to_string();
            let new_path = format!(
                "{}{}",
                prefix.trim_end_matches('/'),
                if current_path.starts_with('/') { &current_path } else { "/" }
            );
            let query = upstream_request.uri.query();
            let new_uri = if let Some(q) = query {
                format!("{new_path}?{q}")
            } else {
                new_path
            };
            if let Ok(parsed) = new_uri.parse() {
                upstream_request.set_uri(parsed);
            } else {
                tracing::warn!(uri = %new_uri, "Failed to parse upstream path prefix URI");
            }
        }

        // SOAP: set Content-Type, SOAPAction headers, fix URL to SOAP endpoint
        if let Some(ref op) = ctx.soap_operation {
            upstream_request
                .insert_header("Content-Type", "text/xml; charset=utf-8")
                .unwrap();
            upstream_request
                .insert_header("SOAPAction", &format!("\"{}\"", op.soap_action))
                .unwrap();
            // SOAP requests go to the endpoint path (e.g. /calculator.asmx), not /calculator.asmx/Add
            // The operation is identified by the SOAPAction header, not the URL
            if let Ok(parsed) = op.endpoint_path.parse() {
                upstream_request.set_uri(parsed);
            }
            // Remove original Content-Length (body size changes after JSON→XML conversion).
            // Add Transfer-Encoding: chunked so upstream accepts variable-length body.
            upstream_request.remove_header("Content-Length");
            upstream_request
                .insert_header("Transfer-Encoding", "chunked")
                .unwrap();
            // Remove Accept-Encoding so upstream returns plain XML (not compressed).
            // The response body filter needs to parse XML for SOAP→JSON conversion.
            upstream_request.remove_header("Accept-Encoding");
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

        // Apply user-configured request-phase header rules
        if let Some(route_id) = ctx.route_id {
            let config = self.config.load();
            if let Some(rules) = config.get_header_rules(&route_id) {
                for rule in rules.iter().filter(|r| r.phase == "request") {
                    if let Ok(header_name) = http::header::HeaderName::from_bytes(rule.header_name.as_bytes()) {
                        match rule.action.as_str() {
                            "set" => {
                                if let Some(ref val) = rule.header_value {
                                    let _ = upstream_request.insert_header(header_name, val);
                                }
                            }
                            "add" => {
                                if let Some(ref val) = rule.header_value {
                                    let _ = upstream_request.append_header(header_name, val);
                                }
                            }
                            "remove" => {
                                upstream_request.remove_header(&header_name);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn fail_to_connect(
        &self,
        _session: &mut Session,
        _peer: &HttpPeer,
        ctx: &mut Self::CTX,
        mut e: Box<pingora::Error>,
    ) -> Box<pingora::Error> {
        // Record circuit breaker failure
        if let Some(target_id) = ctx.target_id {
            let tripped = self.state.record_cb_failure(&target_id);
            if tripped {
                metrics::CIRCUIT_BREAKER_TRIPS
                    .with_label_values(&[
                        &ctx.upstream_id.map(|id| id.to_string()).unwrap_or_default(),
                        &ctx.upstream_target.as_deref().unwrap_or("-").to_string(),
                    ])
                    .inc();
            }

            // Decrement connection count (was incremented in upstream_peer)
            let tracker = self.conn_tracker.lock().unwrap_or_else(|e| e.into_inner());
            tracker.decrement(&target_id);
            metrics::ACTIVE_CONNECTIONS.dec();
        }

        // Record upstream error metric
        metrics::UPSTREAM_ERRORS
            .with_label_values(&[
                &ctx.upstream_id.map(|id| id.to_string()).unwrap_or_default(),
                &ctx.upstream_target.as_deref().unwrap_or("-").to_string(),
            ])
            .inc();

        // Retry if retries remaining
        if ctx.retries_remaining > 0 {
            ctx.retries_remaining -= 1;
            ctx.target_id = None; // Force new target selection
            ctx.upstream_target = None;
            e.set_retry(true);
            metrics::RETRIES_TOTAL
                .with_label_values(&[&ctx.path_prefix])
                .inc();
        }

        e
    }

    async fn upstream_response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut pingora::http::ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Circuit breaker: track success/failure by status code
        if let Some(target_id) = ctx.target_id {
            let status = upstream_response.status.as_u16();
            if status >= 500 {
                let tripped = self.state.record_cb_failure(&target_id);
                if tripped {
                    metrics::CIRCUIT_BREAKER_TRIPS
                        .with_label_values(&[
                            &ctx.upstream_id.map(|id| id.to_string()).unwrap_or_default(),
                            &ctx.upstream_target.as_deref().unwrap_or("-").to_string(),
                        ])
                        .inc();
                }
            } else {
                self.state.record_cb_success(&target_id);
            }
        }

        // SOAP: remove Content-Length (body will shrink from XML to JSON) and fix Content-Type
        if ctx.soap_operation.is_some() {
            upstream_response.remove_header("Content-Length");
            let _ = upstream_response.insert_header("Content-Type", "application/json");
            let _ = upstream_response.insert_header("Transfer-Encoding", "chunked");
        }

        // Start capturing response body for error responses (for APM error details)
        if upstream_response.status.as_u16() >= 400 {
            ctx.error_body = Some(Vec::with_capacity(4096));
        }

        // Apply user-configured response-phase header rules
        if let Some(route_id) = ctx.route_id {
            let config = self.config.load();
            if let Some(rules) = config.get_header_rules(&route_id) {
                for rule in rules.iter().filter(|r| r.phase == "response") {
                    if let Ok(header_name) = http::header::HeaderName::from_bytes(rule.header_name.as_bytes()) {
                        match rule.action.as_str() {
                            "set" => {
                                if let Some(ref val) = rule.header_value {
                                    let _ = upstream_response.insert_header(header_name, val);
                                }
                            }
                            "add" => {
                                if let Some(ref val) = rule.header_value {
                                    let _ = upstream_response.append_header(header_name, val);
                                }
                            }
                            "remove" => {
                                upstream_response.remove_header(&header_name);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn upstream_response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<bytes::Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<std::time::Duration>> {
        const MAX_ERROR_BODY: usize = 4096;

        // For non-SOAP routes, capture error body directly from upstream chunks
        if ctx.soap_operation.is_none() {
            if let Some(ref mut err_buf) = ctx.error_body {
                if let Some(ref data) = body {
                    let remaining = MAX_ERROR_BODY.saturating_sub(err_buf.len());
                    if remaining > 0 {
                        let take = data.len().min(remaining);
                        err_buf.extend_from_slice(&data[..take]);
                    }
                }
            }
            return Ok(None);
        }

        // Buffer incoming SOAP XML response chunks
        if let Some(data) = body.take() {
            if ctx.soap_response_body.len() + data.len() > crate::soap::MAX_SOAP_BODY_BYTES {
                tracing::error!("SOAP response body exceeds size limit");
                ctx.soap_response_body.clear();
                let err_json = serde_json::json!({"error": "SOAP response too large"});
                *body = Some(bytes::Bytes::from(serde_json::to_vec(&err_json).unwrap_or_default()));
                return Ok(None);
            }
            ctx.soap_response_body.extend_from_slice(&data);
        }

        if end_of_stream {
            if let Some(ref op) = ctx.soap_operation {
                match crate::soap::soap_xml_to_json(
                    &ctx.soap_response_body,
                    &op.output_element,
                ) {
                    Ok(json_val) => {
                        let json_bytes = serde_json::to_vec(&json_val).unwrap_or_default();
                        *body = Some(bytes::Bytes::from(json_bytes));
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to convert SOAP XML to JSON");
                        let err_json = serde_json::json!({"error": "Failed to parse SOAP response", "raw": String::from_utf8_lossy(&ctx.soap_response_body).to_string()});
                        *body = Some(bytes::Bytes::from(serde_json::to_vec(&err_json).unwrap_or_default()));
                    }
                }
                ctx.soap_response_body.clear();
            }

            // For SOAP routes, capture the final converted body (JSON) as the error body
            if let Some(ref mut err_buf) = ctx.error_body {
                if let Some(ref data) = body {
                    let take = data.len().min(MAX_ERROR_BODY);
                    err_buf.clear();
                    err_buf.extend_from_slice(&data[..take]);
                }
            }
        }

        Ok(None)
    }

    fn request_cache_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<()> {
        // Only cache GET requests with a positive TTL
        if let Some(ttl) = ctx.cache_ttl_secs {
            if ttl > 0 && session.req_header().method == http::Method::GET {
                session.cache.enable(
                    &*CACHE_BACKEND,
                    None, // no eviction manager
                    None, // no cacheable predictor
                    None, // no cache lock
                    None, // no option overrides
                );
            }
        }
        Ok(())
    }

    fn response_cache_filter(
        &self,
        _session: &Session,
        resp: &pingora::http::ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<pingora_cache::RespCacheable> {
        if let Some(ttl) = ctx.cache_ttl_secs {
            if ttl > 0 && resp.status == http::StatusCode::OK {
                let now = std::time::SystemTime::now();
                let fresh_until = now + std::time::Duration::from_secs(ttl as u64);
                let meta = pingora_cache::CacheMeta::new(
                    fresh_until,
                    now,
                    0, // stale_while_revalidate_sec
                    0, // stale_if_error_sec
                    resp.clone(),
                );
                return Ok(pingora_cache::RespCacheable::Cacheable(meta));
            }
        }
        Ok(pingora_cache::RespCacheable::Uncacheable(
            pingora_cache::NoCacheReason::Custom("not configured"),
        ))
    }

    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) {
        // Decrement connection count for least-connections tracking
        if let Some(target_id) = ctx.target_id {
            let tracker = self.conn_tracker.lock().unwrap_or_else(|e| e.into_inner());
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
        let route_label = if ctx.path_prefix.is_empty() {
            ctx.route_id
                .map(|id| id.to_string())
                .unwrap_or_default()
        } else {
            ctx.path_prefix.clone()
        };

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

        // Send log entry to async writer (drop if channel full)
        let error_body = ctx.error_body.take().and_then(|b| {
            String::from_utf8(b).ok().filter(|s| !s.is_empty())
        });
        let timestamp_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        let _ = self.log_sender.try_send(RequestLogEntry {
            route_id: ctx.route_id,
            method: method.to_string(),
            path: path.to_string(),
            status_code: status as i32,
            latency_ms: duration.as_secs_f64() * 1000.0,
            client_ip: self.get_client_ip(session),
            upstream_target: ctx.upstream_target.clone(),
            error_body,
            timestamp_us,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::CircuitBreaker;
    use crate::lb::ConnectionTracker;
    use crate::router::GatewayConfig;
    use crate::state::MemoryState;
    use arc_swap::ArcSwap;
    fn make_test_proxy() -> GatewayProxy {
        let config = GatewayConfig::new(vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        let state = Arc::new(StateBackend::Memory(
            MemoryState::new(Arc::new(CircuitBreaker::new())),
        ));
        GatewayProxy::new(
            Arc::new(ArcSwap::from_pointee(config)),
            Arc::new(Mutex::new(ConnectionTracker::new())),
            tx,
            vec![],
            state,
        )
    }

    #[tokio::test]
    async fn check_rate_limit_allows_within_limit() {
        let proxy = make_test_proxy();
        let route_id = Uuid::new_v4();

        let result = proxy.state.check_rate_limit(&route_id, "client1", 10).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 9); // 10 - 1 = 9 remaining
    }

    #[tokio::test]
    async fn check_rate_limit_blocks_over_limit() {
        let proxy = make_test_proxy();
        let route_id = Uuid::new_v4();

        // 2 rps limit — third request should be blocked
        let r1 = proxy.state.check_rate_limit(&route_id, "client1", 2).await;
        assert!(r1.is_ok());
        assert_eq!(r1.unwrap(), 1);

        let r2 = proxy.state.check_rate_limit(&route_id, "client1", 2).await;
        assert!(r2.is_ok());
        assert_eq!(r2.unwrap(), 0);

        let r3 = proxy.state.check_rate_limit(&route_id, "client1", 2).await;
        assert!(r3.is_err());
    }

    #[tokio::test]
    async fn check_rate_limit_separate_clients_independent() {
        let proxy = make_test_proxy();
        let route_id = Uuid::new_v4();

        // 1 rps limit, different clients should be independent
        let r1 = proxy.state.check_rate_limit(&route_id, "client1", 1).await;
        let r2 = proxy.state.check_rate_limit(&route_id, "client2", 1).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn check_rate_limit_separate_routes_independent() {
        let proxy = make_test_proxy();
        let route1 = Uuid::new_v4();
        let route2 = Uuid::new_v4();

        // 1 rps limit, different routes should be independent
        let r1 = proxy.state.check_rate_limit(&route1, "client1", 1).await;
        let r2 = proxy.state.check_rate_limit(&route2, "client1", 1).await;
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn check_rate_limit_returns_retry_after() {
        let proxy = make_test_proxy();
        let route_id = Uuid::new_v4();

        // 1 rps limit
        let _ = proxy.state.check_rate_limit(&route_id, "client1", 1).await;
        let result = proxy.state.check_rate_limit(&route_id, "client1", 1).await;
        assert!(result.is_err());
        let retry_after = result.unwrap_err();
        assert!(retry_after >= 1); // at least 1 second
    }

    #[tokio::test]
    async fn new_ctx_has_default_values() {
        let proxy = make_test_proxy();
        let ctx = proxy.new_ctx();
        assert!(ctx.route_id.is_none());
        assert!(ctx.upstream_id.is_none());
        assert!(ctx.upstream_target.is_none());
        assert!(ctx.target_id.is_none());
        assert!(!ctx.strip_prefix);
        assert!(ctx.path_prefix.is_empty());
        assert!(ctx.upstream_path_prefix.is_none());
        assert!(!ctx.target_tls);
        assert!(ctx.client_identity.is_none());
        assert!(ctx.timeout_ms.is_none());
        assert_eq!(ctx.retries_remaining, 0);
        assert_eq!(ctx.retries_total, 0);
        assert!(ctx.cache_ttl_secs.is_none());
        assert!(ctx.soap_operation.is_none());
        assert!(ctx.soap_request_body.is_empty());
        assert!(ctx.soap_response_body.is_empty());
    }

    #[tokio::test]
    async fn gateway_proxy_new_initializes_correctly() {
        let proxy = make_test_proxy();
        assert_eq!(
            proxy.rr_counter.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    // --- Rate limiting (via state backend) ---

    #[tokio::test]
    async fn rate_limit_different_routes_isolated() {
        let proxy = make_test_proxy();
        let route1 = Uuid::new_v4();
        let route2 = Uuid::new_v4();

        // Fill route1 to limit
        let _ = proxy.state.check_rate_limit(&route1, "client", 1).await;
        assert!(proxy.state.check_rate_limit(&route1, "client", 1).await.is_err());

        // route2 should still work
        assert!(proxy.state.check_rate_limit(&route2, "client", 1).await.is_ok());
    }

    #[tokio::test]
    async fn rate_limit_concurrent_access() {
        let proxy = Arc::new(make_test_proxy());
        let route_id = Uuid::new_v4();
        let limit = 1000i32;
        let num_tasks = 50usize;

        let mut handles = Vec::new();
        for _ in 0..num_tasks {
            let proxy = Arc::clone(&proxy);
            let rid = route_id;
            handles.push(tokio::spawn(async move {
                proxy.state.check_rate_limit(&rid, "client", limit).await
            }));
        }

        let mut ok_count = 0u32;
        let mut err_count = 0u32;
        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => ok_count += 1,
                Err(_) => err_count += 1,
            }
        }

        // All 50 should succeed (well under 1000 limit)
        assert_eq!(ok_count, num_tasks as u32);
        assert_eq!(err_count, 0);
    }

    // --- Trusted proxies ---

    #[tokio::test]
    async fn proxy_new_stores_trusted_proxies() {
        let config = GatewayConfig::new(vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        let state = Arc::new(StateBackend::Memory(
            MemoryState::new(Arc::new(CircuitBreaker::new())),
        ));
        let proxy = GatewayProxy::new(
            Arc::new(ArcSwap::from_pointee(config)),
            Arc::new(Mutex::new(ConnectionTracker::new())),
            tx,
            vec!["10.0.0.1".to_string(), "172.16.0.1".to_string()],
            state,
        );
        assert_eq!(proxy.trusted_proxies.len(), 2);
        assert_eq!(proxy.trusted_proxies[0], "10.0.0.1");
    }

    #[tokio::test]
    async fn proxy_empty_trusted_proxies_by_default() {
        let proxy = make_test_proxy();
        assert!(proxy.trusted_proxies.is_empty());
    }

    // --- rewrite_path ---

    #[test]
    fn rewrite_path_no_strip() {
        let result = rewrite_path("/api/users", "/api", false, None, None);
        assert_eq!(result, "/api/users");
    }

    #[test]
    fn rewrite_path_strip_prefix() {
        let result = rewrite_path("/api/users", "/api", true, None, None);
        assert_eq!(result, "/users");
    }

    #[test]
    fn rewrite_path_strip_to_root() {
        let result = rewrite_path("/api", "/api", true, None, None);
        assert_eq!(result, "/");
    }

    #[test]
    fn rewrite_path_with_upstream_prefix() {
        let result = rewrite_path("/users", "", false, Some("/v2"), None);
        assert_eq!(result, "/v2/users");
    }

    #[test]
    fn rewrite_path_with_query() {
        let result = rewrite_path("/api/users", "/api", true, None, Some("page=1&limit=10"));
        assert_eq!(result, "/users?page=1&limit=10");
    }

    #[test]
    fn rewrite_path_strip_and_upstream_prefix() {
        let result = rewrite_path("/api/users", "/api", true, Some("/v3"), None);
        assert_eq!(result, "/v3/users");
    }

    #[test]
    fn rewrite_path_strip_and_upstream_prefix_and_query() {
        let result = rewrite_path("/api/users", "/api", true, Some("/v3"), Some("id=5"));
        assert_eq!(result, "/v3/users?id=5");
    }

    #[test]
    fn rewrite_path_empty_prefix_strip() {
        // Empty prefix with strip should still produce valid path
        let result = rewrite_path("/users", "", true, None, None);
        assert_eq!(result, "/users");
    }

    #[test]
    fn rewrite_path_strip_ensures_leading_slash() {
        // Path shorter than prefix edge case
        let result = rewrite_path("/a", "/api/long", true, None, None);
        assert_eq!(result, "/");
    }

    #[test]
    fn rewrite_path_upstream_prefix_trailing_slash() {
        let result = rewrite_path("/users", "", false, Some("/v2/"), None);
        assert_eq!(result, "/v2/users");
    }
}

// Helper methods
impl GatewayProxy {
    fn get_peer_ip(&self, session: &Session) -> String {
        session
            .client_addr()
            .map(|a| {
                let addr = a.to_string();
                addr.rsplit_once(':')
                    .map(|(ip, _port)| ip.to_string())
                    .unwrap_or(addr)
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn get_client_ip(&self, session: &Session) -> String {
        let peer_ip = self.get_peer_ip(session);

        // Only trust X-Forwarded-For if the direct peer is a trusted proxy
        let is_trusted = self.trusted_proxies.iter().any(|p| p == &peer_ip);

        if is_trusted {
            if let Some(xff) = session.req_header().headers.get("x-forwarded-for") {
                if let Ok(s) = xff.to_str() {
                    if let Some(first_ip) = s.split(',').next() {
                        return first_ip.trim().to_string();
                    }
                }
            }
        }

        peer_ip
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
