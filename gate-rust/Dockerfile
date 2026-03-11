FROM rust:1.86-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs && cargo build --release && rm -rf src
COPY src/ src/
RUN touch src/main.rs && cargo build --release

FROM alpine:3.21
RUN addgroup -S gate && adduser -S gate -G gate
COPY --from=builder /app/target/release/gate-rust /usr/local/bin/gate-rust
USER gate
EXPOSE 8080
ENTRYPOINT ["gate-rust"]
