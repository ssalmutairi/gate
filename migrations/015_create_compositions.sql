CREATE TABLE IF NOT EXISTS compositions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    path_prefix TEXT NOT NULL,
    path_pattern TEXT,
    methods TEXT[],
    host_pattern TEXT,
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    max_wait_ms INTEGER,
    auth_skip BOOLEAN NOT NULL DEFAULT false,
    active BOOLEAN NOT NULL DEFAULT true,
    response_merge JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS composition_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    composition_id UUID NOT NULL REFERENCES compositions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    step_order INTEGER NOT NULL DEFAULT 0,
    method TEXT NOT NULL DEFAULT 'GET',
    upstream_id UUID NOT NULL REFERENCES upstreams(id),
    path_template TEXT NOT NULL,
    body_template JSONB,
    headers_template JSONB,
    depends_on TEXT[],
    on_error TEXT NOT NULL DEFAULT 'abort',
    default_value JSONB,
    timeout_ms INTEGER NOT NULL DEFAULT 10000,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(composition_id, name)
);

CREATE INDEX IF NOT EXISTS idx_composition_steps_composition_id ON composition_steps(composition_id);
CREATE INDEX IF NOT EXISTS idx_compositions_updated_at ON compositions(updated_at);
CREATE INDEX IF NOT EXISTS idx_composition_steps_updated_at ON composition_steps(updated_at);
