-- Gate standalone SQLite schema

CREATE TABLE IF NOT EXISTS upstreams (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    name TEXT NOT NULL UNIQUE,
    algorithm TEXT NOT NULL DEFAULT 'round_robin',
    circuit_breaker_threshold INTEGER,
    circuit_breaker_duration_secs INTEGER NOT NULL DEFAULT 30,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS targets (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    upstream_id TEXT NOT NULL REFERENCES upstreams(id) ON DELETE CASCADE,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    weight INTEGER NOT NULL DEFAULT 1,
    healthy INTEGER NOT NULL DEFAULT 1,
    tls INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS routes (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    name TEXT NOT NULL,
    path_prefix TEXT NOT NULL,
    methods TEXT,
    upstream_id TEXT NOT NULL REFERENCES upstreams(id),
    strip_prefix INTEGER NOT NULL DEFAULT 0,
    upstream_path_prefix TEXT,
    service_id TEXT,
    max_body_bytes INTEGER,
    timeout_ms INTEGER,
    retries INTEGER NOT NULL DEFAULT 0,
    host_pattern TEXT,
    cache_ttl_secs INTEGER,
    auth_skip INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    route_id TEXT REFERENCES routes(id) ON DELETE CASCADE,
    active INTEGER NOT NULL DEFAULT 1,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS rate_limits (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    route_id TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    requests_per_second INTEGER NOT NULL,
    requests_per_minute INTEGER,
    requests_per_hour INTEGER,
    limit_by TEXT NOT NULL DEFAULT 'ip',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS header_rules (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    route_id TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    phase TEXT NOT NULL DEFAULT 'request',
    action TEXT NOT NULL,
    header_name TEXT NOT NULL,
    header_value TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS ip_rules (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    route_id TEXT NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    cidr TEXT NOT NULL,
    action TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for foreign key lookups and common queries
CREATE INDEX IF NOT EXISTS idx_targets_upstream_id ON targets(upstream_id);
CREATE INDEX IF NOT EXISTS idx_routes_upstream_id ON routes(upstream_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_route_id ON api_keys(route_id);
CREATE INDEX IF NOT EXISTS idx_rate_limits_route_id ON rate_limits(route_id);
CREATE INDEX IF NOT EXISTS idx_header_rules_route_id ON header_rules(route_id);
CREATE INDEX IF NOT EXISTS idx_ip_rules_route_id ON ip_rules(route_id);

CREATE TABLE IF NOT EXISTS services (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    namespace TEXT NOT NULL UNIQUE,
    version INTEGER NOT NULL DEFAULT 1,
    spec_url TEXT NOT NULL,
    spec_hash TEXT NOT NULL,
    upstream_id TEXT NOT NULL REFERENCES upstreams(id),
    route_id TEXT REFERENCES routes(id),
    description TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'stable',
    spec_content TEXT,
    service_type TEXT NOT NULL DEFAULT 'rest',
    soap_metadata TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_services_namespace ON services(namespace);
CREATE INDEX IF NOT EXISTS idx_services_upstream_id ON services(upstream_id);
CREATE INDEX IF NOT EXISTS idx_services_route_id ON services(route_id);

-- Indexes for config reloader change-detection (MAX(updated_at) queries)
CREATE INDEX IF NOT EXISTS idx_routes_updated_at ON routes(updated_at);
CREATE INDEX IF NOT EXISTS idx_upstreams_updated_at ON upstreams(updated_at);
CREATE INDEX IF NOT EXISTS idx_targets_updated_at ON targets(updated_at);
CREATE INDEX IF NOT EXISTS idx_api_keys_updated_at ON api_keys(updated_at);
CREATE INDEX IF NOT EXISTS idx_rate_limits_updated_at ON rate_limits(updated_at);
CREATE INDEX IF NOT EXISTS idx_header_rules_updated_at ON header_rules(updated_at);
CREATE INDEX IF NOT EXISTS idx_ip_rules_updated_at ON ip_rules(updated_at);
