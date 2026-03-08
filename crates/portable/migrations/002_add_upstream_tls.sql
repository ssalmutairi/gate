ALTER TABLE upstreams ADD COLUMN tls_ca_cert TEXT;
ALTER TABLE upstreams ADD COLUMN tls_client_cert TEXT;
ALTER TABLE upstreams ADD COLUMN tls_client_key TEXT;
ALTER TABLE upstreams ADD COLUMN tls_skip_verify INTEGER NOT NULL DEFAULT 0;
