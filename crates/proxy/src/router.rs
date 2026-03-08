use crate::soap::SoapServiceMeta;
use pingora::utils::tls::CertKey;
use shared::models::{ApiKey, HeaderRule, IpRule, RateLimit, Route, Target, Upstream};
use shared::tls::{pem_to_der_certs, pem_to_der_key};
use std::collections::HashMap;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use uuid::Uuid;

/// Pre-parsed TLS materials for an upstream, cached to avoid PEM parsing per request.
#[derive(Debug, Clone)]
pub struct UpstreamTlsConfig {
    pub skip_verify: bool,
    pub client_cert_key: Option<Arc<CertKey>>,
}

/// In-memory snapshot of all gateway config.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Routes sorted by path_prefix length descending (longest first).
    pub routes: Vec<Route>,
    /// Upstreams keyed by ID.
    pub upstreams: HashMap<Uuid, Upstream>,
    /// Targets keyed by upstream ID.
    pub targets: HashMap<Uuid, Vec<Target>>,
    /// API keys: all active keys.
    pub api_keys: Vec<ApiKey>,
    /// Rate limit rules keyed by route ID.
    pub rate_limits: HashMap<Uuid, RateLimit>,
    /// Header modification rules keyed by route ID.
    pub header_rules: HashMap<Uuid, Vec<HeaderRule>>,
    /// IP allowlist/denylist rules keyed by route ID.
    pub ip_rules: HashMap<Uuid, Vec<IpRule>>,
    /// SOAP service metadata keyed by service_id (route's service_id).
    pub soap_services: HashMap<Uuid, SoapServiceMeta>,
    /// Pre-parsed upstream TLS configurations.
    pub upstream_tls: HashMap<Uuid, UpstreamTlsConfig>,
}

impl GatewayConfig {
    #[cfg(test)]
    pub fn new(
        routes: Vec<Route>,
        upstreams: Vec<Upstream>,
        targets: Vec<Target>,
        api_keys: Vec<ApiKey>,
        rate_limits: Vec<RateLimit>,
        header_rules: Vec<HeaderRule>,
        ip_rules: Vec<IpRule>,
    ) -> Self {
        Self::with_soap(routes, upstreams, targets, api_keys, rate_limits, header_rules, ip_rules, HashMap::new())
    }

    pub fn with_soap(
        mut routes: Vec<Route>,
        upstreams: Vec<Upstream>,
        targets: Vec<Target>,
        api_keys: Vec<ApiKey>,
        rate_limits: Vec<RateLimit>,
        header_rules: Vec<HeaderRule>,
        ip_rules: Vec<IpRule>,
        soap_services: HashMap<Uuid, SoapServiceMeta>,
    ) -> Self {
        // Sort routes: longest path_prefix first for most-specific match
        routes.sort_by(|a, b| b.path_prefix.len().cmp(&a.path_prefix.len()));

        let upstreams_map: HashMap<Uuid, Upstream> =
            upstreams.into_iter().map(|u| (u.id, u)).collect();

        let mut targets_map: HashMap<Uuid, Vec<Target>> = HashMap::new();
        for t in targets {
            targets_map.entry(t.upstream_id).or_default().push(t);
        }

        let rate_limits_map: HashMap<Uuid, RateLimit> =
            rate_limits.into_iter().map(|r| (r.route_id, r)).collect();

        let mut header_rules_map: HashMap<Uuid, Vec<HeaderRule>> = HashMap::new();
        for rule in header_rules {
            header_rules_map.entry(rule.route_id).or_default().push(rule);
        }

        let mut ip_rules_map: HashMap<Uuid, Vec<IpRule>> = HashMap::new();
        for rule in ip_rules {
            ip_rules_map.entry(rule.route_id).or_default().push(rule);
        }

        let upstream_tls = build_upstream_tls(&upstreams_map);

        Self {
            routes,
            upstreams: upstreams_map,
            targets: targets_map,
            api_keys,
            rate_limits: rate_limits_map,
            header_rules: header_rules_map,
            ip_rules: ip_rules_map,
            soap_services,
            upstream_tls,
        }
    }

    /// Match an incoming request path, method, and optional host against configured routes.
    pub fn match_route(&self, path: &str, method: &str, host: Option<&str>) -> Option<&Route> {
        for route in &self.routes {
            if !route.active {
                continue;
            }

            // Check host pattern if the route has one
            if let Some(ref pattern) = route.host_pattern {
                match host {
                    Some(h) => {
                        if !host_matches(h, pattern) {
                            continue;
                        }
                    }
                    None => continue, // Route requires a host but none provided
                }
            }

            if !path.starts_with(&route.path_prefix) {
                continue;
            }

            // Ensure the match is at a segment boundary
            let rest = &path[route.path_prefix.len()..];
            if !rest.is_empty() && !rest.starts_with('/') {
                continue;
            }

            if let Some(ref methods) = route.methods {
                if !methods.is_empty()
                    && !methods.iter().any(|m| m.eq_ignore_ascii_case(method))
                {
                    continue;
                }
            }

            return Some(route);
        }
        None
    }

    /// Get healthy targets for a given upstream ID.
    pub fn healthy_targets(&self, upstream_id: &Uuid) -> Vec<&Target> {
        self.targets
            .get(upstream_id)
            .map(|targets| targets.iter().filter(|t| t.healthy).collect())
            .unwrap_or_default()
    }

    /// Check if a route requires authentication (any API keys scoped to it or global exist).
    /// A route requires auth if ANY keys exist for it, regardless of active status.
    pub fn route_requires_auth(&self, route_id: &Uuid) -> bool {
        self.api_keys.iter().any(|k| {
            k.route_id.is_none() || k.route_id.as_ref() == Some(route_id)
        })
    }

    /// Validate an API key for a given route.
    /// Returns the key identity (hash) on success, or an error message.
    pub fn validate_api_key(
        &self,
        route_id: &Uuid,
        key_hash: &str,
    ) -> Result<String, &'static str> {
        for key in &self.api_keys {
            // Check scope: key must be global or match the route
            if key.route_id.is_some() && key.route_id.as_ref() != Some(route_id) {
                continue;
            }

            // Constant-time comparison
            if !constant_time_eq(key.key_hash.as_bytes(), key_hash.as_bytes()) {
                continue;
            }

            // Found a matching key — check if active
            if !key.active {
                return Err("API key has been revoked");
            }

            // Check expiry
            if let Some(expires_at) = key.expires_at {
                if chrono::Utc::now() > expires_at {
                    return Err("API key has expired");
                }
            }

            return Ok(key.key_hash.clone());
        }

        Err("Invalid API key")
    }

    /// Get rate limit rule for a route.
    pub fn get_rate_limit(&self, route_id: &Uuid) -> Option<&RateLimit> {
        self.rate_limits.get(route_id)
    }

    /// Get header rules for a route.
    pub fn get_header_rules(&self, route_id: &Uuid) -> Option<&Vec<HeaderRule>> {
        self.header_rules.get(route_id)
    }

    /// Get IP rules for a route.
    pub fn get_ip_rules(&self, route_id: &Uuid) -> Option<&Vec<IpRule>> {
        self.ip_rules.get(route_id)
    }

    /// Get SOAP service metadata for a route by looking up its service_id.
    pub fn get_soap_meta(&self, route: &Route) -> Option<&SoapServiceMeta> {
        route.service_id.as_ref().and_then(|sid| self.soap_services.get(sid))
    }
}

/// Build pre-parsed TLS configs for upstreams that have TLS settings.
fn build_upstream_tls(upstreams: &HashMap<Uuid, Upstream>) -> HashMap<Uuid, UpstreamTlsConfig> {
    let mut map = HashMap::new();
    for (id, upstream) in upstreams {
        let has_tls_config = upstream.tls_skip_verify
            || upstream.tls_client_cert.is_some();

        if !has_tls_config {
            continue;
        }

        let client_cert_key = match (&upstream.tls_client_cert, &upstream.tls_client_key) {
            (Some(cert_pem), Some(key_pem)) => {
                let certs = pem_to_der_certs(cert_pem);
                let key = pem_to_der_key(key_pem);
                match (certs.is_empty(), key) {
                    (false, Some(key_der)) => Some(Arc::new(CertKey::new(certs, key_der))),
                    _ => {
                        tracing::warn!(upstream_id = %id, "Failed to parse client cert/key PEM");
                        None
                    }
                }
            }
            _ => None,
        };

        map.insert(*id, UpstreamTlsConfig {
            skip_verify: upstream.tls_skip_verify,
            client_cert_key,
        });
    }
    map
}

/// Check if a host matches a pattern. Supports exact match and wildcard `*.example.com`.
fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();

    if let Some(suffix) = pattern.strip_prefix("*.") {
        // Wildcard: host must end with .suffix and have at least one char before
        host.ends_with(&format!(".{}", suffix)) && host.len() > suffix.len() + 1
    } else {
        host == pattern
    }
}

/// Constant-time byte comparison to prevent timing attacks.
/// Uses subtle::ConstantTimeEq to avoid leaking length information.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    fn empty_config() -> GatewayConfig {
        GatewayConfig::new(vec![], vec![], vec![], vec![], vec![], vec![], vec![])
    }

    // --- match_route ---

    #[test]
    fn match_route_exact_prefix() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![], vec![]);
        let matched = cfg.match_route("/api", "GET", None);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r.id);
    }

    #[test]
    fn match_route_with_subpath() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![], vec![]);
        let matched = cfg.match_route("/api/users", "GET", None);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r.id);
    }

    #[test]
    fn match_route_longest_prefix_wins() {
        let u = make_upstream();
        let r1 = make_route(u.id, "/api");
        let r2 = make_route(u.id, "/api/users");
        let cfg = GatewayConfig::new(
            vec![r1, r2.clone()],
            vec![u],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let matched = cfg.match_route("/api/users/123", "GET", None);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r2.id);
    }

    #[test]
    fn match_route_segment_boundary() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![], vec![]);
        // "/api-v2" should NOT match "/api" — not a segment boundary
        assert!(cfg.match_route("/api-v2", "GET", None).is_none());
    }

    #[test]
    fn match_route_inactive_skipped() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.active = false;
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "GET", None).is_none());
    }

    #[test]
    fn match_route_method_filter() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.methods = Some(vec!["GET".to_string(), "POST".to_string()]);
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "GET", None).is_some());
        assert!(cfg.match_route("/api", "POST", None).is_some());
        assert!(cfg.match_route("/api", "DELETE", None).is_none());
    }

    #[test]
    fn match_route_case_insensitive_method() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.methods = Some(vec!["GET".to_string()]);
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "get", None).is_some());
        assert!(cfg.match_route("/api", "Get", None).is_some());
    }

    #[test]
    fn match_route_no_match() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/other", "GET", None).is_none());
    }

    #[test]
    fn match_route_root_exact() {
        let u = make_upstream();
        let r = make_route(u.id, "/");
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![], vec![]);
        // Root route "/" matches "/" exactly
        let matched = cfg.match_route("/", "GET", None);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r.id);
    }

    #[test]
    fn match_route_empty_methods_matches_all() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.methods = Some(vec![]);
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "DELETE", None).is_some());
    }

    // --- host-based routing ---

    #[test]
    fn match_route_host_exact() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.host_pattern = Some("example.com".to_string());
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "GET", Some("example.com")).is_some());
        assert!(cfg.match_route("/api", "GET", Some("other.com")).is_none());
        assert!(cfg.match_route("/api", "GET", None).is_none());
    }

    #[test]
    fn match_route_host_wildcard() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.host_pattern = Some("*.example.com".to_string());
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "GET", Some("app.example.com")).is_some());
        assert!(cfg.match_route("/api", "GET", Some("example.com")).is_none());
        assert!(cfg.match_route("/api", "GET", Some("other.com")).is_none());
    }

    #[test]
    fn match_route_host_no_pattern() {
        let u = make_upstream();
        let r = make_route(u.id, "/api"); // no host_pattern
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![], vec![]);
        // Routes without host_pattern match any host
        assert!(cfg.match_route("/api", "GET", Some("anything.com")).is_some());
        assert!(cfg.match_route("/api", "GET", None).is_some());
    }

    // --- healthy_targets ---

    #[test]
    fn healthy_targets_only_healthy_returned() {
        let u = make_upstream();
        let mut t1 = make_target(u.id);
        t1.healthy = true;
        let mut t2 = make_target(u.id);
        t2.healthy = false;
        let cfg = GatewayConfig::new(vec![], vec![u.clone()], vec![t1.clone(), t2], vec![], vec![], vec![], vec![]);
        let healthy = cfg.healthy_targets(&u.id);
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].id, t1.id);
    }

    #[test]
    fn healthy_targets_empty_for_unknown_upstream() {
        let cfg = empty_config();
        let healthy = cfg.healthy_targets(&Uuid::new_v4());
        assert!(healthy.is_empty());
    }

    #[test]
    fn healthy_targets_empty_when_all_unhealthy() {
        let u = make_upstream();
        let mut t1 = make_target(u.id);
        t1.healthy = false;
        let mut t2 = make_target(u.id);
        t2.healthy = false;
        let cfg = GatewayConfig::new(vec![], vec![u.clone()], vec![t1, t2], vec![], vec![], vec![], vec![]);
        assert!(cfg.healthy_targets(&u.id).is_empty());
    }

    // --- route_requires_auth ---

    #[test]
    fn route_requires_auth_global_key() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(None, "hash123"); // global key
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert!(cfg.route_requires_auth(&route_id));
    }

    #[test]
    fn route_requires_auth_scoped_key() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(Some(route_id), "hash123");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert!(cfg.route_requires_auth(&route_id));
    }

    #[test]
    fn route_requires_auth_no_keys() {
        let cfg = empty_config();
        assert!(!cfg.route_requires_auth(&Uuid::new_v4()));
    }

    // --- validate_api_key ---

    #[test]
    fn validate_api_key_valid() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(Some(route_id), "correcthash");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert!(cfg.validate_api_key(&route_id, "correcthash").is_ok());
    }

    #[test]
    fn validate_api_key_wrong_hash() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(Some(route_id), "correcthash");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert_eq!(
            cfg.validate_api_key(&route_id, "wronghash"),
            Err("Invalid API key")
        );
    }

    #[test]
    fn validate_api_key_revoked() {
        let route_id = Uuid::new_v4();
        let mut key = make_api_key(Some(route_id), "hash123");
        key.active = false;
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert_eq!(
            cfg.validate_api_key(&route_id, "hash123"),
            Err("API key has been revoked")
        );
    }

    #[test]
    fn validate_api_key_expired() {
        let route_id = Uuid::new_v4();
        let mut key = make_api_key(Some(route_id), "hash123");
        key.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert_eq!(
            cfg.validate_api_key(&route_id, "hash123"),
            Err("API key has expired")
        );
    }

    #[test]
    fn validate_api_key_wrong_route_scope() {
        let route_a = Uuid::new_v4();
        let route_b = Uuid::new_v4();
        let key = make_api_key(Some(route_a), "hash123");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert_eq!(
            cfg.validate_api_key(&route_b, "hash123"),
            Err("Invalid API key")
        );
    }

    // --- get_rate_limit ---

    #[test]
    fn get_rate_limit_found() {
        let route_id = Uuid::new_v4();
        let rl = make_rate_limit(route_id, 100);
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![], vec![rl.clone()], vec![], vec![]);
        let result = cfg.get_rate_limit(&route_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().requests_per_second, 100);
    }

    #[test]
    fn get_rate_limit_not_found() {
        let cfg = empty_config();
        assert!(cfg.get_rate_limit(&Uuid::new_v4()).is_none());
    }

    // --- get_header_rules ---

    #[test]
    fn get_header_rules_found() {
        let route_id = Uuid::new_v4();
        let rule = make_header_rule(route_id, "set", "X-Custom");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![], vec![], vec![rule], vec![]);
        let result = cfg.get_header_rules(&route_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn get_header_rules_not_found() {
        let cfg = empty_config();
        assert!(cfg.get_header_rules(&Uuid::new_v4()).is_none());
    }

    // --- constant_time_eq edge cases (via validate_api_key) ---

    #[test]
    fn validate_api_key_different_length_hash_rejected() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(Some(route_id), "abcdef1234567890abcdef1234567890");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        // Short hash triggers constant_time_eq different-length fast path
        assert_eq!(
            cfg.validate_api_key(&route_id, "short"),
            Err("Invalid API key")
        );
    }

    // --- GatewayConfig::new edge cases ---

    #[test]
    fn config_new_sorts_routes_by_prefix_length() {
        let u = make_upstream();
        let r_short = make_route(u.id, "/a");
        let r_long = make_route(u.id, "/a/b/c");
        let r_mid = make_route(u.id, "/a/b");
        // Pass in unsorted order
        let cfg = GatewayConfig::new(
            vec![r_short.clone(), r_long.clone(), r_mid.clone()],
            vec![u],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        // Routes should be sorted longest first
        assert_eq!(cfg.routes[0].id, r_long.id);
        assert_eq!(cfg.routes[1].id, r_mid.id);
        assert_eq!(cfg.routes[2].id, r_short.id);
    }

    #[test]
    fn config_new_groups_targets_by_upstream() {
        let u1 = make_upstream();
        let u2 = make_upstream();
        let t1 = make_target(u1.id);
        let t2 = make_target(u1.id);
        let t3 = make_target(u2.id);
        let cfg = GatewayConfig::new(
            vec![],
            vec![u1.clone(), u2.clone()],
            vec![t1.clone(), t2.clone(), t3.clone()],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(cfg.targets.get(&u1.id).unwrap().len(), 2);
        assert_eq!(cfg.targets.get(&u2.id).unwrap().len(), 1);
    }

    #[test]
    fn validate_api_key_global_key_matches_any_route() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(None, "globalhash"); // global key (route_id = None)
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert!(cfg.validate_api_key(&route_id, "globalhash").is_ok());
    }

    #[test]
    fn validate_api_key_not_yet_expired() {
        let route_id = Uuid::new_v4();
        let mut key = make_api_key(Some(route_id), "hash123");
        key.expires_at = Some(chrono::Utc::now() + chrono::Duration::hours(1));
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        assert!(cfg.validate_api_key(&route_id, "hash123").is_ok());
    }

    #[test]
    fn route_requires_auth_scoped_key_different_route() {
        let route_a = Uuid::new_v4();
        let route_b = Uuid::new_v4();
        // Key scoped to route_a only
        let key = make_api_key(Some(route_a), "hash");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![], vec![]);
        // route_b should NOT require auth (no global keys, no keys scoped to route_b)
        assert!(!cfg.route_requires_auth(&route_b));
    }

    // --- constant_time_eq (subtle::ConstantTimeEq) ---

    #[test]
    fn constant_time_eq_same_length_same_content() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn constant_time_eq_same_length_different_content() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn constant_time_eq_different_length() {
        // subtle::ct_eq returns false for different lengths
        assert!(!constant_time_eq(b"short", b"muchlonger"));
    }

    #[test]
    fn constant_time_eq_empty_strings() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_one_empty_one_not() {
        assert!(!constant_time_eq(b"", b"notempty"));
        assert!(!constant_time_eq(b"notempty", b""));
    }

    #[test]
    fn constant_time_eq_single_byte_difference() {
        assert!(!constant_time_eq(b"aaaa", b"aaab"));
    }
}
