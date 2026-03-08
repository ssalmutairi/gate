use crate::proxy_core::soap::SoapServiceMeta;
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

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub routes: Vec<Route>,
    pub upstreams: HashMap<Uuid, Upstream>,
    pub targets: HashMap<Uuid, Vec<Target>>,
    pub api_keys: Vec<ApiKey>,
    pub rate_limits: HashMap<Uuid, RateLimit>,
    pub header_rules: HashMap<Uuid, Vec<HeaderRule>>,
    pub ip_rules: HashMap<Uuid, Vec<IpRule>>,
    pub soap_services: HashMap<Uuid, SoapServiceMeta>,
    pub upstream_tls: HashMap<Uuid, UpstreamTlsConfig>,
}

impl GatewayConfig {
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

    pub fn match_route(&self, path: &str, method: &str, host: Option<&str>) -> Option<&Route> {
        for route in &self.routes {
            if !route.active {
                continue;
            }

            if let Some(ref pattern) = route.host_pattern {
                match host {
                    Some(h) => {
                        if !host_matches(h, pattern) {
                            continue;
                        }
                    }
                    None => continue,
                }
            }

            if !path.starts_with(&route.path_prefix) {
                continue;
            }

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

    pub fn healthy_targets(&self, upstream_id: &Uuid) -> Vec<&Target> {
        self.targets
            .get(upstream_id)
            .map(|targets| targets.iter().filter(|t| t.healthy).collect())
            .unwrap_or_default()
    }

    pub fn route_requires_auth(&self, route_id: &Uuid) -> bool {
        self.api_keys.iter().any(|k| {
            k.route_id.is_none() || k.route_id.as_ref() == Some(route_id)
        })
    }

    pub fn validate_api_key(
        &self,
        route_id: &Uuid,
        key_hash: &str,
    ) -> Result<String, &'static str> {
        for key in &self.api_keys {
            if key.route_id.is_some() && key.route_id.as_ref() != Some(route_id) {
                continue;
            }

            if !constant_time_eq(key.key_hash.as_bytes(), key_hash.as_bytes()) {
                continue;
            }

            if !key.active {
                return Err("API key has been revoked");
            }

            if let Some(expires_at) = key.expires_at {
                if chrono::Utc::now() > expires_at {
                    return Err("API key has expired");
                }
            }

            return Ok(key.key_hash.clone());
        }

        Err("Invalid API key")
    }

    pub fn get_rate_limit(&self, route_id: &Uuid) -> Option<&RateLimit> {
        self.rate_limits.get(route_id)
    }

    pub fn get_header_rules(&self, route_id: &Uuid) -> Option<&Vec<HeaderRule>> {
        self.header_rules.get(route_id)
    }

    pub fn get_ip_rules(&self, route_id: &Uuid) -> Option<&Vec<IpRule>> {
        self.ip_rules.get(route_id)
    }

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

fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();

    if let Some(suffix) = pattern.strip_prefix("*.") {
        host.ends_with(&format!(".{}", suffix)) && host.len() > suffix.len() + 1
    } else {
        host == pattern
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}
