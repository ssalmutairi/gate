use arc_swap::ArcSwap;
use lb::ConnectionTracker;
use pingora::prelude::*;
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, Mutex};

mod circuit_breaker;
mod config;
mod health;
mod lb;
mod logging;
mod metrics;
mod router;
mod service;

#[cfg(test)]
mod test_helpers;

use service::GatewayProxy;
use shared::config::AppConfig;

fn main() {
    // Install rustls crypto provider for TLS upstream connections
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let app_config = AppConfig::from_env();

    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&app_config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    print_banner(&app_config);

    // Create a Tokio runtime for async DB setup
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime for setup");

    // Initialize DB pool and load initial config
    let (db_pool, gateway_config) = rt.block_on(async {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&app_config.database_url)
            .await
            .expect("Failed to connect to database");

        tracing::info!("Database connected");

        let cfg = config::load_config(&pool).await;
        (Arc::new(pool), Arc::new(ArcSwap::from_pointee(cfg)))
    });

    // Shared connection tracker for least-connections algorithm
    let conn_tracker = Arc::new(Mutex::new(ConnectionTracker::new()));

    // Circuit breaker state
    let circuit_breaker = Arc::new(circuit_breaker::CircuitBreaker::new());

    // Configure circuit breakers from initial config
    {
        let cfg = gateway_config.load();
        for (upstream_id, upstream) in &cfg.upstreams {
            if let Some(threshold) = upstream.circuit_breaker_threshold {
                if let Some(targets) = cfg.targets.get(upstream_id) {
                    for target in targets {
                        circuit_breaker.configure(
                            target.id,
                            threshold as u32,
                            upstream.circuit_breaker_duration_secs as u32,
                        );
                    }
                }
            }
        }
    }

    // Spawn config reloader in a background thread with its own runtime and DB pool
    config::spawn_config_reloader(
        app_config.database_url.clone(),
        gateway_config.clone(),
        app_config.config_poll_interval_secs,
        circuit_breaker.clone(),
    );

    // Spawn health checker in a background thread with its own runtime and DB pool
    let health_config = gateway_config.clone();
    let health_tracker = conn_tracker.clone();
    let health_db_url = app_config.database_url.clone();
    let health_interval = app_config.health_check_interval_secs;
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build health check runtime");

        rt.block_on(async move {
            let pool = PgPoolOptions::new()
                .max_connections(2)
                .connect(&health_db_url)
                .await
                .expect("Health checker failed to connect to database");

            health::run_health_checks(&pool, health_config, health_tracker, health_interval).await;
        });
    });

    // Spawn async log writer
    let log_sender = logging::spawn_log_writer(app_config.database_url.clone());

    // Initialize Prometheus metrics and start metrics server
    metrics::init();
    metrics::spawn_metrics_server(app_config.metrics_port);

    // Create Pingora server
    let mut server = Server::new(None).expect("Failed to create Pingora server");
    server.bootstrap();

    // Create the proxy service
    let proxy = GatewayProxy::new(
        db_pool,
        gateway_config,
        conn_tracker,
        log_sender,
        app_config.trusted_proxies.clone(),
        circuit_breaker,
    );
    let mut proxy_service = http_proxy_service(&server.configuration, proxy);

    let addr = format!("0.0.0.0:{}", app_config.proxy_port);
    proxy_service.add_tcp(&addr);
    tracing::info!(addr = %addr, "Proxy listening");

    server.add_service(proxy_service);
    server.run_forever();
}

fn print_banner(config: &AppConfig) {
    let proxy_addr = format!("0.0.0.0:{}", config.proxy_port);
    let metrics_addr = format!("0.0.0.0:{}", config.metrics_port);
    let reload = format!("every {}s", config.config_poll_interval_secs);
    let health = format!("every {}s", config.health_check_interval_secs);
    eprintln!();
    eprintln!("  ┌───────────────────────────────────┐");
    eprintln!("  │     Gate Proxy v{}      │", env!("CARGO_PKG_VERSION"));
    eprintln!("  ├───────────────────────────────────┤");
    eprintln!("  │  Proxy:   {:<24}│", proxy_addr);
    eprintln!("  │  Metrics: {:<24}│", metrics_addr);
    eprintln!("  │  Reload:  {:<24}│", reload);
    eprintln!("  │  Health:  {:<24}│", health);
    eprintln!("  └───────────────────────────────────┘");
    eprintln!();
}
