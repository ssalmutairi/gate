-- Services table: tracks imported OpenAPI/Swagger specs
CREATE TABLE IF NOT EXISTS services (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    namespace VARCHAR(255) NOT NULL UNIQUE,
    version INTEGER NOT NULL DEFAULT 1,
    spec_url TEXT NOT NULL,
    spec_hash VARCHAR(64) NOT NULL,
    upstream_id UUID NOT NULL REFERENCES upstreams(id) ON DELETE CASCADE,
    route_id UUID REFERENCES routes(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Route: upstream_path_prefix for path rewriting, service_id to track ownership
ALTER TABLE routes ADD COLUMN IF NOT EXISTS upstream_path_prefix VARCHAR(255) DEFAULT NULL;
ALTER TABLE routes ADD COLUMN IF NOT EXISTS service_id UUID REFERENCES services(id) ON DELETE SET NULL;

-- Target: TLS flag for HTTPS upstreams
ALTER TABLE targets ADD COLUMN IF NOT EXISTS tls BOOLEAN NOT NULL DEFAULT false;
