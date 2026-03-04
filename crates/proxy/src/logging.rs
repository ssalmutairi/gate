use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

/// A log entry to be batched and inserted into the database.
#[derive(Debug)]
pub struct RequestLogEntry {
    pub route_id: Option<Uuid>,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub latency_ms: f64,
    pub client_ip: String,
    pub upstream_target: Option<String>,
}

/// Maximum number of pending log entries before new entries are dropped.
const LOG_CHANNEL_CAPACITY: usize = 10_000;

/// Creates a log sender channel and spawns the background batch writer.
/// Returns the sender that can be used to enqueue log entries.
pub fn spawn_log_writer(database_url: String) -> mpsc::Sender<RequestLogEntry> {
    let (tx, rx) = mpsc::channel(LOG_CHANNEL_CAPACITY);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build log writer runtime");

        rt.block_on(async move {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(3)
                .connect(&database_url)
                .await
                .expect("Log writer failed to connect to database");

            batch_writer(pool, rx).await;
        });
    });

    tx
}

/// Background task that batches log entries and inserts them periodically.
pub(crate) async fn batch_writer(pool: PgPool, mut rx: mpsc::Receiver<RequestLogEntry>) {
    let mut buffer: Vec<RequestLogEntry> = Vec::with_capacity(100);
    let flush_interval = Duration::from_secs(1);

    loop {
        // Wait for entries or flush timeout
        let deadline = tokio::time::sleep(flush_interval);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                entry = rx.recv() => {
                    match entry {
                        Some(e) => {
                            buffer.push(e);
                            if buffer.len() >= 100 {
                                break;
                            }
                        }
                        None => {
                            // Channel closed, flush remaining and exit
                            if !buffer.is_empty() {
                                flush_batch(&pool, &mut buffer).await;
                            }
                            return;
                        }
                    }
                }
                _ = &mut deadline => {
                    break;
                }
            }
        }

        if !buffer.is_empty() {
            flush_batch(&pool, &mut buffer).await;
        }
    }
}

/// Flush a batch of log entries into the database.
pub(crate) async fn flush_batch(pool: &PgPool, buffer: &mut Vec<RequestLogEntry>) {
    // Build a multi-row INSERT
    let mut query = String::from(
        "INSERT INTO request_logs (route_id, method, path, status_code, latency_ms, client_ip, upstream_target) VALUES ",
    );
    let mut params: Vec<String> = Vec::new();

    for (i, _) in buffer.iter().enumerate() {
        let offset = i * 7;
        params.push(format!(
            "(${}, ${}, ${}, ${}, ${}, ${}, ${})",
            offset + 1,
            offset + 2,
            offset + 3,
            offset + 4,
            offset + 5,
            offset + 6,
            offset + 7
        ));
    }
    query.push_str(&params.join(", "));

    let mut q = sqlx::query(&query);
    for entry in buffer.iter() {
        q = q
            .bind(entry.route_id)
            .bind(&entry.method)
            .bind(&entry.path)
            .bind(entry.status_code)
            .bind(entry.latency_ms)
            .bind(&entry.client_ip)
            .bind(&entry.upstream_target);
    }

    if let Err(e) = q.execute(pool).await {
        tracing::error!(count = buffer.len(), error = %e, "Failed to flush request logs");
    }

    buffer.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Mutex;

    /// Serialize DB tests to avoid parallel TRUNCATE races on request_logs.
    static DB_LOCK: Mutex<()> = Mutex::new(());

    async fn setup_test_pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://gate:gate@localhost:5555/gate_test".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        let migrations = [
            include_str!("../../../migrations/001_create_upstreams.sql"),
            include_str!("../../../migrations/002_create_targets.sql"),
            include_str!("../../../migrations/003_create_routes.sql"),
            include_str!("../../../migrations/004_create_api_keys.sql"),
            include_str!("../../../migrations/005_create_rate_limits.sql"),
            include_str!("../../../migrations/006_create_request_logs.sql"),
            include_str!("../../../migrations/007_create_services.sql"),
            include_str!("../../../migrations/008_add_route_max_body.sql"),
            include_str!("../../../migrations/009_add_spec_content.sql"),
            include_str!("../../../migrations/010_add_route_auth_skip.sql"),
        ];
        for sql in &migrations {
            for statement in sql.split(';') {
                let trimmed = statement.trim();
                if trimmed.is_empty() { continue; }
                let _ = sqlx::query(trimmed).execute(&pool).await;
            }
        }

        // Clear request_logs
        sqlx::query("TRUNCATE TABLE request_logs CASCADE")
            .execute(&pool).await.unwrap();
        pool
    }

    fn make_entry(method: &str, path: &str, status: i32) -> RequestLogEntry {
        RequestLogEntry {
            route_id: None,
            method: method.to_string(),
            path: path.to_string(),
            status_code: status,
            latency_ms: 1.5,
            client_ip: "127.0.0.1".to_string(),
            upstream_target: Some("127.0.0.1:8080".to_string()),
        }
    }

    #[tokio::test]
    async fn flush_batch_inserts_rows() {
        let _lock = DB_LOCK.lock().unwrap();
        let pool = setup_test_pool().await;
        let mut buffer = vec![make_entry("GET", "/test", 200)];
        flush_batch(&pool, &mut buffer).await;
        assert!(buffer.is_empty());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs")
            .fetch_one(&pool).await.unwrap();
        assert!(count.0 >= 1);
    }

    #[tokio::test]
    async fn flush_batch_multiple_entries() {
        let _lock = DB_LOCK.lock().unwrap();
        let pool = setup_test_pool().await;
        let mut buffer = vec![
            make_entry("GET", "/a", 200),
            make_entry("POST", "/b", 201),
            make_entry("DELETE", "/c", 204),
        ];
        flush_batch(&pool, &mut buffer).await;
        assert!(buffer.is_empty());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs")
            .fetch_one(&pool).await.unwrap();
        assert!(count.0 >= 3);
    }

    #[tokio::test]
    async fn batch_writer_flushes_on_channel_close() {
        let _lock = DB_LOCK.lock().unwrap();
        let pool = setup_test_pool().await;
        let (tx, rx) = mpsc::channel(100);

        // Send some entries then drop the sender
        tx.send(make_entry("GET", "/batch-close", 200)).await.unwrap();
        tx.send(make_entry("POST", "/batch-close", 201)).await.unwrap();
        drop(tx);

        // batch_writer should process remaining entries and return
        batch_writer(pool.clone(), rx).await;

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE path = '/batch-close'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn batch_writer_empty_channel_exits() {
        let _lock = DB_LOCK.lock().unwrap();
        let pool = setup_test_pool().await;
        let (_tx, rx) = mpsc::channel::<RequestLogEntry>(100);

        // Drop sender immediately — batch_writer should exit cleanly
        drop(_tx);
        batch_writer(pool, rx).await;
        // If we get here, it exited properly
    }
}
