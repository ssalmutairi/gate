use shared::models::{ApiKey, HeaderRule, RateLimit, Route, Target, Upstream};
use std::collections::HashMap;
use subtle::ConstantTimeEq;
use uuid::Uuid;

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
}

impl GatewayConfig {
    pub fn new(
        mut routes: Vec<Route>,
        upstreams: Vec<Upstream>,
        targets: Vec<Target>,
        api_keys: Vec<ApiKey>,
        rate_limits: Vec<RateLimit>,
        header_rules: Vec<HeaderRule>,
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

        Self {
            routes,
            upstreams: upstreams_map,
            targets: targets_map,
            api_keys,
            rate_limits: rate_limits_map,
            header_rules: header_rules_map,
        }
    }

    /// Match an incoming request path and method against configured routes.
    pub fn match_route(&self, path: &str, method: &str) -> Option<&Route> {
        for route in &self.routes {
            if !route.active {
                continue;
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
        GatewayConfig::new(vec![], vec![], vec![], vec![], vec![], vec![])
    }

    // --- match_route ---

    #[test]
    fn match_route_exact_prefix() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![]);
        let matched = cfg.match_route("/api", "GET");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r.id);
    }

    #[test]
    fn match_route_with_subpath() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![]);
        let matched = cfg.match_route("/api/users", "GET");
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
        );
        let matched = cfg.match_route("/api/users/123", "GET");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r2.id);
    }

    #[test]
    fn match_route_segment_boundary() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![]);
        // "/api-v2" should NOT match "/api" — not a segment boundary
        assert!(cfg.match_route("/api-v2", "GET").is_none());
    }

    #[test]
    fn match_route_inactive_skipped() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.active = false;
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "GET").is_none());
    }

    #[test]
    fn match_route_method_filter() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.methods = Some(vec!["GET".to_string(), "POST".to_string()]);
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "GET").is_some());
        assert!(cfg.match_route("/api", "POST").is_some());
        assert!(cfg.match_route("/api", "DELETE").is_none());
    }

    #[test]
    fn match_route_case_insensitive_method() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.methods = Some(vec!["GET".to_string()]);
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "get").is_some());
        assert!(cfg.match_route("/api", "Get").is_some());
    }

    #[test]
    fn match_route_no_match() {
        let u = make_upstream();
        let r = make_route(u.id, "/api");
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/other", "GET").is_none());
    }

    #[test]
    fn match_route_root_exact() {
        let u = make_upstream();
        let r = make_route(u.id, "/");
        let cfg = GatewayConfig::new(vec![r.clone()], vec![u], vec![], vec![], vec![], vec![]);
        // Root route "/" matches "/" exactly
        let matched = cfg.match_route("/", "GET");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().id, r.id);
    }

    #[test]
    fn match_route_empty_methods_matches_all() {
        let u = make_upstream();
        let mut r = make_route(u.id, "/api");
        r.methods = Some(vec![]);
        let cfg = GatewayConfig::new(vec![r], vec![u], vec![], vec![], vec![], vec![]);
        assert!(cfg.match_route("/api", "DELETE").is_some());
    }

    // --- healthy_targets ---

    #[test]
    fn healthy_targets_only_healthy_returned() {
        let u = make_upstream();
        let mut t1 = make_target(u.id);
        t1.healthy = true;
        let mut t2 = make_target(u.id);
        t2.healthy = false;
        let cfg = GatewayConfig::new(vec![], vec![u.clone()], vec![t1.clone(), t2], vec![], vec![], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![u.clone()], vec![t1, t2], vec![], vec![], vec![]);
        assert!(cfg.healthy_targets(&u.id).is_empty());
    }

    // --- route_requires_auth ---

    #[test]
    fn route_requires_auth_global_key() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(None, "hash123"); // global key
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
        assert!(cfg.route_requires_auth(&route_id));
    }

    #[test]
    fn route_requires_auth_scoped_key() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(Some(route_id), "hash123");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
        assert!(cfg.validate_api_key(&route_id, "correcthash").is_ok());
    }

    #[test]
    fn validate_api_key_wrong_hash() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(Some(route_id), "correcthash");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![], vec![rl.clone()], vec![]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![], vec![], vec![rule]);
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
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
        );
        assert_eq!(cfg.targets.get(&u1.id).unwrap().len(), 2);
        assert_eq!(cfg.targets.get(&u2.id).unwrap().len(), 1);
    }

    #[test]
    fn validate_api_key_global_key_matches_any_route() {
        let route_id = Uuid::new_v4();
        let key = make_api_key(None, "globalhash"); // global key (route_id = None)
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
        assert!(cfg.validate_api_key(&route_id, "globalhash").is_ok());
    }

    #[test]
    fn validate_api_key_not_yet_expired() {
        let route_id = Uuid::new_v4();
        let mut key = make_api_key(Some(route_id), "hash123");
        key.expires_at = Some(chrono::Utc::now() + chrono::Duration::hours(1));
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
        assert!(cfg.validate_api_key(&route_id, "hash123").is_ok());
    }

    #[test]
    fn route_requires_auth_scoped_key_different_route() {
        let route_a = Uuid::new_v4();
        let route_b = Uuid::new_v4();
        // Key scoped to route_a only
        let key = make_api_key(Some(route_a), "hash");
        let cfg = GatewayConfig::new(vec![], vec![], vec![], vec![key], vec![], vec![]);
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
