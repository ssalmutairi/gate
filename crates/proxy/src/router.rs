use shared::models::{ApiKey, RateLimit, Route, Target, Upstream};
use std::collections::HashMap;
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
}

impl GatewayConfig {
    pub fn new(
        mut routes: Vec<Route>,
        upstreams: Vec<Upstream>,
        targets: Vec<Target>,
        api_keys: Vec<ApiKey>,
        rate_limits: Vec<RateLimit>,
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

        Self {
            routes,
            upstreams: upstreams_map,
            targets: targets_map,
            api_keys,
            rate_limits: rate_limits_map,
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
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}
