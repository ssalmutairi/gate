-- Feature 1: Request size limiting
ALTER TABLE routes ADD COLUMN IF NOT EXISTS max_body_bytes BIGINT DEFAULT NULL;

-- Feature 2: Header modification rules
CREATE TABLE IF NOT EXISTS header_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    route_id UUID NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    phase VARCHAR(20) NOT NULL DEFAULT 'request',
    action VARCHAR(20) NOT NULL,
    header_name VARCHAR(255) NOT NULL,
    header_value VARCHAR(1024) DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Feature 3: API versioning / service metadata
ALTER TABLE services ADD COLUMN IF NOT EXISTS description TEXT DEFAULT '';
ALTER TABLE services ADD COLUMN IF NOT EXISTS tags TEXT[] DEFAULT '{}';
ALTER TABLE services ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'stable';
