CREATE TABLE routes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    path_prefix VARCHAR(255) NOT NULL,
    methods TEXT[] DEFAULT NULL,
    upstream_id UUID NOT NULL REFERENCES upstreams(id),
    strip_prefix BOOLEAN NOT NULL DEFAULT false,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
