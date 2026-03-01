CREATE TABLE rate_limits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    route_id UUID NOT NULL REFERENCES routes(id) ON DELETE CASCADE,
    requests_per_second INTEGER NOT NULL,
    requests_per_minute INTEGER DEFAULT NULL,
    requests_per_hour INTEGER DEFAULT NULL,
    limit_by VARCHAR(50) NOT NULL DEFAULT 'ip',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
