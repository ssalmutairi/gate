/// Application configuration parsed from environment variables.
pub struct AppConfig {
    pub database_url: String,
    pub proxy_port: u16,
    pub admin_port: u16,
    pub admin_bind_addr: String,
    pub admin_token: Option<String>,
    pub log_level: String,
    pub config_poll_interval_secs: u64,
    pub health_check_interval_secs: u64,
    pub health_check_path: String,
    pub metrics_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            proxy_port: std::env::var("PROXY_PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .expect("PROXY_PORT must be a valid port number"),
            admin_port: std::env::var("ADMIN_PORT")
                .unwrap_or_else(|_| "9000".into())
                .parse()
                .expect("ADMIN_PORT must be a valid port number"),
            admin_bind_addr: std::env::var("ADMIN_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1".into()),
            admin_token: std::env::var("ADMIN_TOKEN").ok(),
            log_level: std::env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "info".into()),
            config_poll_interval_secs: std::env::var("CONFIG_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "5".into())
                .parse()
                .expect("CONFIG_POLL_INTERVAL_SECS must be a number"),
            health_check_interval_secs: std::env::var("HEALTH_CHECK_INTERVAL_SECS")
                .unwrap_or_else(|_| "10".into())
                .parse()
                .expect("HEALTH_CHECK_INTERVAL_SECS must be a number"),
            health_check_path: std::env::var("HEALTH_CHECK_PATH")
                .unwrap_or_else(|_| "/health".into()),
            metrics_port: std::env::var("METRICS_PORT")
                .unwrap_or_else(|_| "9091".into())
                .parse()
                .expect("METRICS_PORT must be a valid port number"),
        }
    }
}
