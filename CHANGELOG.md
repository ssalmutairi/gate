# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.6.3] - 2026-03-06

### Fixed

- **WSDL Complex Type Resolution** — WSDL parser now resolves standalone named `complexType` definitions into nested object schemas; fields referencing complex types were incorrectly shown as `string` instead of expanded objects with their child fields
- **Self-Closing XSD Elements** — Self-closing `<xs:element ... />` tags at schema level no longer break parsing of subsequent standalone complex types
- **Nested SOAP XML** — Proxy JSON→SOAP XML builder now recursively writes nested JSON objects as XML elements instead of serializing them as raw JSON strings

## [1.6.2] - 2026-03-05

### Fixed

- **Large Spec Upload** — Request body limit now uses the configurable `MAX_SPEC_SIZE_MB` (default 25 MB) instead of a hardcoded 1 MB cap; large WSDL and OpenAPI file uploads were rejected before the handler could process them

## [1.6.1] - 2026-03-05

### Fixed

- **Missing SOAP Migration** — Added `014_add_soap_support.sql` to the migration runner; WSDL imports were failing with "column service_type does not exist" because the migration file was not registered in `db.rs`
- **SOAP Proxy Content Encoding** — Strip `Accept-Encoding` header on SOAP upstream requests so the response arrives as plain XML; compressed responses caused `ERR_CONTENT_DECODING_FAILED` in the browser because the SOAP→JSON body filter cannot parse brotli/gzip-encoded XML
- **X-Api-Key Header Support** — Proxy now accepts API keys via `X-Api-Key` header in addition to `Authorization: Bearer`, matching the dashboard's Try It panel which sends keys as `X-Api-Key`

## [1.6.0] - 2026-03-05

### Added

- **SOAP/WSDL Support** — Import WSDL services via URL or file upload; the gateway automatically parses WSDL operations, creates SOAP routes, and converts JSON requests to SOAP XML and SOAP XML responses back to JSON
- **SOAP-to-JSON Proxy** — Transparent JSON↔SOAP translation on the proxy hot path: clients send/receive JSON while the upstream sees standard SOAP envelopes with correct SOAPAction headers
- **WSDL Parser** — Extracts operations, SOAPAction URIs, input/output element names, and the SOAP endpoint URL from WSDL documents (supports WSDL 1.1)
- **Elastic APM Logging Backend** — Optional Elastic APM as an alternative to PostgreSQL for request logging (`ELASTIC_APM_ENABLED=true`), sending NDJSON batches to the APM Intake V2 API with transaction and error events
- **APM Error Events** — Failed requests (status >= 400) emit dedicated APM error events with upstream response body (up to 4 KB), including HTML-to-plaintext stripping for readable error messages in Kibana
- **Error Body Capture** — Upstream error response bodies are captured (4 KB cap) and included in APM error events for debugging; SOAP routes capture the post-conversion JSON, non-SOAP routes capture the raw upstream body
- **OpenAPI Schema Display** — Service detail page shows resolved request body and response schemas for each endpoint, with recursive `$ref` resolution supporting both OpenAPI 3.x and Swagger 2.0 specs
- **Try It Panel** — Interactive endpoint testing from the dashboard with pre-filled request bodies from schema examples
- **Gateway Dev Proxy** — Vite dev server proxies `/gateway` requests to the proxy server for CORS-free endpoint testing
- **Redis State Backend** — Optional Redis-backed distributed rate limiting and circuit breaker sync for multi-instance deployments (`--features redis-backend`)
- **Helm Chart** — Full Helm chart (`charts/gate/`) with Bitnami PostgreSQL/Redis subcharts, separate proxy and admin deployments
- **Plain Kubernetes Manifests** — Ready-to-use YAML manifests (`deploy/kubernetes/`) with PostgreSQL StatefulSet, Redis, and Kustomize support

### Changed

- **Prometheus Metric Labels** — Route metrics now use the path prefix (e.g. `/petstore`) instead of UUIDs
- **Logging Backend Selection** — `spawn_log_writer` now accepts a `LogBackend` enum (Postgres or ElasticApm) instead of a raw database URL; batch writer loop deduplicated via macro
- **URL Validation** — Import dialog validates URL format client-side only (avoids CORS errors); reachability is checked server-side during import
- **Circuit Breaker API** — `record_success()` returns `bool` for HalfOpen→Closed transition; Redis publish only fires on state transitions
- **Rate Limiter Optimization** — Single `entry().or_insert_with()` call instead of double DashMap lookup

### Fixed

- **Redis Pool Size** — `REDIS_POOL_SIZE` config is now actually applied to the deadpool-redis pool builder
- **Flaky Logging Tests** — DB logging tests now use unique path filters instead of `COUNT(*)` on the whole table
- **Config Test Stability** — Removed assertions that conflict with `.env` file values loaded by dotenvy

## [1.5.0] - 2026-03-02

### Added

- **Response Compression** — gzip/brotli/zstd compression via Pingora `ResponseCompression` module (level 6), automatic `Accept-Encoding` negotiation
- **Host-Based Routing** — Route matching by `Host` header with exact match and wildcard patterns (`*.example.com`)
- **IP Allowlist/Denylist** — Per-route CIDR-based IP rules (allow/deny) with admin CRUD endpoints
- **Response Caching** — Per-route TTL-based in-memory response caching via Pingora cache API for GET requests
- **Standard Auth Header** — Proxy authentication now uses `Authorization: Bearer <token>` instead of custom `X-Api-Key` header

### Fixed

- **API Key Display** — Dashboard now shows the generated API key in a modal with copy-to-clipboard after creation (was silently discarding the key)
- **Scope Default** — API key creation form defaults to "Global (all routes)" instead of ambiguous empty placeholder
- **Global API Key Creation** — Fixed dashboard sending `__global__` as `route_id` instead of null, causing UUID parse error on the backend

### Changed

- **Dead Code Cleanup** — Removed unused `db_pool` field from `GatewayProxy`, removed unused `get_upstream_cb_config` method, gated test-only functions with `#[cfg(test)]`

## [1.4.0] - 2026-03-02

### Changed

- **Lock-Free Rate Limiter** — Replaced `Mutex<HashMap>` rate limiter with `DashMap<String, AtomicU64>` and atomic CAS counters, eliminating the global lock that serialized every rate-limited request at high concurrency
- **Fixed-Window Counters** — Switched from sliding window (Vec of timestamps) to fixed-window counters: O(1) per request with zero allocations, matching the approach used by nginx/Envoy

## [1.3.0] - 2026-03-02

### Added

- **Embedded Dashboard** — Dashboard UI is now compiled directly into the admin binary via `rust-embed`; `gate-admin` serves both API and UI on a single port
- **Cross-Platform Releases** — GitHub Actions workflow builds precompiled binaries for Linux (x86_64, aarch64) and macOS (x86_64, aarch64) on every version tag push
- **Install Script** — One-liner `curl | bash` installer with OS/arch detection, SHA256 checksum verification, and configurable install directory
- **Cross-Compilation Support** — `Cross.toml` config for building aarch64-linux targets with cmake support for Pingora

### Changed

- **Single-Binary Admin** — Admin binary now serves the dashboard on `/` as a fallback; no separate nginx container needed
- **Docker Simplified** — Dockerfile uses a Node.js build stage to embed the dashboard; removed standalone `dashboard` service from `docker-compose.yml`
- **Dynamic Version Banner** — Admin startup banner now reads version from `Cargo.toml` via `env!("CARGO_PKG_VERSION")` instead of a hardcoded string

## [1.2.0] - 2026-03-02

### Added

- **Settings Page** — Dedicated settings page with theme selection, timezone configuration, and app version display
- **Timezone Support** — User-selectable timezone (default: Asia/Riyadh) persisted to localStorage; all dates/times across Logs, API Keys, and Services pages respect the chosen timezone via `Intl.DateTimeFormat`

### Changed

- **Sidebar Cleanup** — Moved theme picker and version info from sidebar footer to the new Settings page, keeping sidebar focused on navigation

## [1.0.1] - 2026-03-02

### Fixed

- **TLS Upstream Proxying** - Enabled rustls TLS backend for Pingora with HTTP/2 ALPN negotiation, fixing 502 errors on all HTTPS upstream connections
- **Docker Build** - Fixed glibc mismatch by pinning builder image to `rust:1-slim-bookworm` to match the `debian:bookworm-slim` runtime
- **OpenAPI Import** - Handle relative server URLs in OpenAPI specs by resolving against the source URL
- **Dashboard Dev Proxy** - Corrected Vite dev proxy target port from 9000 to 9001

## [1.0.0] - 2026-03-01

### Added

- **Core Proxy** - Pingora-based reverse proxy with dynamic routing and path prefix matching
- **Admin API** - Full CRUD REST API (Axum) for managing routes, upstreams, targets, API keys, and rate limits
- **Load Balancing** - Round robin, weighted round robin, and least connections algorithms
- **API Key Authentication** - SHA-256 hashed keys with route scoping, active/inactive toggle
- **Rate Limiting** - Per-route sliding window rate limits by IP or API key (per-second/minute/hour)
- **Health Checks** - Background upstream target health monitoring with automatic failover
- **Hot Reload** - Configuration changes polled from PostgreSQL and applied without restart (ArcSwap)
- **Prometheus Metrics** - 7 metric types: request counters, latency histograms, error counters, active connections, health gauges
- **Request Logging** - Async batched request logging to PostgreSQL via tokio mpsc channel
- **React Dashboard** - Full management UI with routes, upstreams, API keys, rate limits, logs, and stats pages
- **Stats & Logs API** - Aggregated statistics (p95 latency, error rate) and paginated request logs
- **Docker Compose** - One-command deployment of full stack (PostgreSQL, Gateway, Dashboard, Prometheus, Grafana)
- **Grafana Dashboards** - Pre-provisioned traffic overview dashboard
- **Security Hardening** - Admin API bound to 127.0.0.1 by default, CORS support, 1MB body limit, X-Forwarded headers, admin token stripping
- **Graceful Shutdown** - SIGTERM/SIGINT handling for clean shutdown
- **Startup Banners** - Configuration summary printed on boot for both proxy and admin binaries
