use shared::config::AppConfig;

#[tokio::main]
async fn main() {
    let config = AppConfig::from_env();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let bind = format!("{}:{}", config.admin_bind_addr, config.admin_port);
    let auth = if config.admin_token.is_some() { "token required" } else { "open (no token)" };
    eprintln!();
    eprintln!("  ┌───────────────────────────────────┐");
    let version = env!("CARGO_PKG_VERSION");
    eprintln!("  │  Gate Admin API v{:<17}│", version);
    eprintln!("  ├───────────────────────────────────┤");
    eprintln!("  │  Bind: {:<27}│", bind);
    eprintln!("  │  Auth: {:<27}│", auth);
    eprintln!("  └───────────────────────────────────┘");
    eprintln!();

    tracing::info!(admin_port = config.admin_port, "Starting Gate admin API");

    let pool = admin::db::create_pool(&config.database_url).await;
    admin::db::run_migrations(&pool).await;

    let app = admin::build_router_with_config(pool, config.max_spec_size_mb * 1024 * 1024);

    let addr = format!("{}:{}", config.admin_bind_addr, config.admin_port);
    tracing::info!(addr = %addr, "Admin API listening");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, stopping admin API...");
}
