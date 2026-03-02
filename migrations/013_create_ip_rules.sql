CREATE TABLE IF NOT EXISTS ip_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    route_id UUID NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    cidr VARCHAR(50) NOT NULL,
    action VARCHAR(10) NOT NULL CHECK (action IN ('allow', 'deny')),
    description VARCHAR(255) DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ip_rules_route_id ON ip_rules(route_id);
