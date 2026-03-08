CREATE TABLE IF NOT EXISTS compositions (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    name TEXT NOT NULL UNIQUE,
    path_prefix TEXT NOT NULL,
    path_pattern TEXT,
    methods TEXT,
    host_pattern TEXT,
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    max_wait_ms INTEGER,
    auth_skip INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    response_merge TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS composition_steps (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6)))),
    composition_id TEXT NOT NULL REFERENCES compositions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    step_order INTEGER NOT NULL DEFAULT 0,
    method TEXT NOT NULL DEFAULT 'GET',
    upstream_id TEXT NOT NULL REFERENCES upstreams(id),
    path_template TEXT NOT NULL,
    body_template TEXT,
    headers_template TEXT,
    depends_on TEXT,
    on_error TEXT NOT NULL DEFAULT 'abort',
    default_value TEXT,
    timeout_ms INTEGER NOT NULL DEFAULT 10000,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(composition_id, name)
);

CREATE INDEX IF NOT EXISTS idx_composition_steps_composition_id ON composition_steps(composition_id);
CREATE INDEX IF NOT EXISTS idx_compositions_updated_at ON compositions(updated_at);
CREATE INDEX IF NOT EXISTS idx_composition_steps_updated_at ON composition_steps(updated_at);
