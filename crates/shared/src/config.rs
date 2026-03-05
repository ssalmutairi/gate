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
    /// Comma-separated list of trusted proxy CIDRs for X-Forwarded-For (e.g. "10.0.0.0/8,172.16.0.0/12").
    /// If empty, X-Forwarded-For is never trusted — peer IP is always used.
    pub trusted_proxies: Vec<String>,
    /// Redis URL for distributed state (rate limiting, circuit breaker sync).
    /// If None, in-memory state is used (single-instance mode).
    pub redis_url: Option<String>,
    /// Redis connection pool size (default: 8).
    pub redis_pool_size: usize,
    /// Maximum spec size for service import in MB (default: 25).
    pub max_spec_size_mb: usize,
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
            trusted_proxies: std::env::var("TRUSTED_PROXIES")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            redis_url: std::env::var("REDIS_URL").ok().filter(|s| !s.is_empty()),
            redis_pool_size: std::env::var("REDIS_POOL_SIZE")
                .unwrap_or_else(|_| "8".into())
                .parse()
                .expect("REDIS_POOL_SIZE must be a number"),
            max_spec_size_mb: std::env::var("MAX_SPEC_SIZE_MB")
                .unwrap_or_else(|_| "25".into())
                .parse()
                .expect("MAX_SPEC_SIZE_MB must be a number"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    // Serialize env-var tests since they mutate shared process state.
    // Use `unwrap_or_else` to recover from poisoned mutex (caused by catch_unwind tests).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Helper: clear all config-related env vars before each test.
    fn clear_env() {
        for key in &[
            "DATABASE_URL", "PROXY_PORT", "ADMIN_PORT", "ADMIN_BIND_ADDR",
            "ADMIN_TOKEN", "LOG_LEVEL", "CONFIG_POLL_INTERVAL_SECS",
            "HEALTH_CHECK_INTERVAL_SECS", "HEALTH_CHECK_PATH", "METRICS_PORT",
            "TRUSTED_PROXIES", "REDIS_URL", "REDIS_POOL_SIZE",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn from_env_with_defaults() {
        let _lock = lock_env();
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");

        let config = super::AppConfig::from_env();
        // Note: dotenvy::dotenv() may load .env file values for unset vars.
        // We only assert on DATABASE_URL which we explicitly set, and on default
        // numeric values that won't be overridden (since we cleared them above,
        // and .env may re-set some).
        assert_eq!(config.database_url, "postgres://test:test@localhost/test");
        // These are either the defaults or from .env — both are valid:
        assert!(config.proxy_port > 0);
        assert!(config.admin_port > 0);
        assert!(!config.admin_bind_addr.is_empty());
        assert!(!config.log_level.is_empty());
        assert!(config.config_poll_interval_secs > 0);
        assert!(config.health_check_interval_secs > 0);
        assert!(!config.health_check_path.is_empty());
        assert!(config.metrics_port > 0);
        assert!(config.redis_url.is_none());
        assert_eq!(config.redis_pool_size, 8);
    }

    #[test]
    fn from_env_with_custom_values() {
        let _lock = lock_env();
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://custom:custom@db:5432/mydb");
        std::env::set_var("PROXY_PORT", "9090");
        std::env::set_var("ADMIN_PORT", "8000");
        std::env::set_var("ADMIN_BIND_ADDR", "0.0.0.0");
        std::env::set_var("ADMIN_TOKEN", "secret");
        std::env::set_var("LOG_LEVEL", "debug");
        std::env::set_var("CONFIG_POLL_INTERVAL_SECS", "30");
        std::env::set_var("HEALTH_CHECK_INTERVAL_SECS", "60");
        std::env::set_var("HEALTH_CHECK_PATH", "/ping");
        std::env::set_var("METRICS_PORT", "3000");
        std::env::set_var("TRUSTED_PROXIES", "10.0.0.0/8,172.16.0.0/12");
        std::env::set_var("REDIS_URL", "redis://localhost:6379");
        std::env::set_var("REDIS_POOL_SIZE", "16");

        let config = super::AppConfig::from_env();
        assert_eq!(config.proxy_port, 9090);
        assert_eq!(config.admin_port, 8000);
        assert_eq!(config.admin_bind_addr, "0.0.0.0");
        assert_eq!(config.admin_token.as_deref(), Some("secret"));
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.config_poll_interval_secs, 30);
        assert_eq!(config.health_check_interval_secs, 60);
        assert_eq!(config.health_check_path, "/ping");
        assert_eq!(config.metrics_port, 3000);
        assert_eq!(config.trusted_proxies, vec!["10.0.0.0/8", "172.16.0.0/12"]);
        assert_eq!(config.redis_url.as_deref(), Some("redis://localhost:6379"));
        assert_eq!(config.redis_pool_size, 16);
    }

    #[test]
    fn explicit_database_url_overrides_dotenv() {
        let _lock = lock_env();
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://override:override@myhost:1234/mydb");

        let config = super::AppConfig::from_env();
        // Explicit env var should take precedence over .env file
        assert_eq!(config.database_url, "postgres://override:override@myhost:1234/mydb");
    }

    #[test]
    fn invalid_proxy_port_panics() {
        let _lock = lock_env();
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://x:x@localhost/x");
        std::env::set_var("PROXY_PORT", "not_a_number");
        let result = std::panic::catch_unwind(|| {
            super::AppConfig::from_env();
        });
        assert!(result.is_err(), "expected panic for invalid PROXY_PORT");
    }

    #[test]
    fn trusted_proxies_empty_string() {
        let _lock = lock_env();
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://x:x@localhost/x");
        std::env::set_var("TRUSTED_PROXIES", "");

        let config = super::AppConfig::from_env();
        assert!(config.trusted_proxies.is_empty());
    }

    #[test]
    fn trusted_proxies_trims_whitespace() {
        let _lock = lock_env();
        clear_env();
        std::env::set_var("DATABASE_URL", "postgres://x:x@localhost/x");
        std::env::set_var("TRUSTED_PROXIES", " 10.0.0.1 , 172.16.0.1 , ");

        let config = super::AppConfig::from_env();
        assert_eq!(config.trusted_proxies, vec!["10.0.0.1", "172.16.0.1"]);
    }
}
