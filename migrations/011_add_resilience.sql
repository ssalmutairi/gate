-- Routes: per-route timeout and retry config
ALTER TABLE routes ADD COLUMN timeout_ms INTEGER;
ALTER TABLE routes ADD COLUMN retries INTEGER NOT NULL DEFAULT 0;

-- Upstreams: circuit breaker config
ALTER TABLE upstreams ADD COLUMN circuit_breaker_threshold INTEGER;
ALTER TABLE upstreams ADD COLUMN circuit_breaker_duration_secs INTEGER NOT NULL DEFAULT 30;
