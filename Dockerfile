# Stage 1: Build dashboard
FROM node:22-alpine AS dashboard-builder

WORKDIR /app/dashboard
COPY dashboard/package.json dashboard/package-lock.json ./
RUN npm ci
COPY dashboard/ ./
RUN npm run build

# Stage 2: Build Rust binaries
FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev cmake && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY migrations/ migrations/
COPY --from=dashboard-builder /app/dashboard/dist dashboard/dist

RUN cargo build --release --bin proxy --bin admin --bin standalone --features redis-backend

# Stage 3: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false gate && \
    mkdir -p /data && chown gate:gate /data

COPY --from=builder /app/target/release/proxy /usr/local/bin/gate-proxy
COPY --from=builder /app/target/release/admin /usr/local/bin/gate-admin
COPY --from=builder /app/target/release/standalone /usr/local/bin/gate-standalone
COPY migrations/ /app/migrations/
COPY entrypoint.sh /app/entrypoint.sh

USER gate
WORKDIR /app

EXPOSE 8080 9000 9090 9091

VOLUME ["/data"]

HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
  CMD curl -f http://localhost:9000/admin/health || exit 1

CMD ["/app/entrypoint.sh"]
