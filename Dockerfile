# Stage 1: Build
FROM rust:1-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev cmake && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY migrations/ migrations/

RUN cargo build --release --bin proxy --bin admin

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false gate

COPY --from=builder /app/target/release/proxy /usr/local/bin/gate-proxy
COPY --from=builder /app/target/release/admin /usr/local/bin/gate-admin
COPY migrations/ /app/migrations/
COPY entrypoint.sh /app/entrypoint.sh

USER gate
WORKDIR /app

EXPOSE 8080 9000 9091

HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
  CMD curl -f http://localhost:9000/admin/health || exit 1

CMD ["/app/entrypoint.sh"]
