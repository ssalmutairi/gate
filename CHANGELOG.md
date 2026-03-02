# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
