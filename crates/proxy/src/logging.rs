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

/// Creates a log sender channel and spawns the background batch writer.
/// Returns the sender that can be used to enqueue log entries.
pub fn spawn_log_writer(database_url: String) -> mpsc::UnboundedSender<RequestLogEntry> {
    let (tx, rx) = mpsc::unbounded_channel();

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
async fn batch_writer(pool: PgPool, mut rx: mpsc::UnboundedReceiver<RequestLogEntry>) {
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
async fn flush_batch(pool: &PgPool, buffer: &mut Vec<RequestLogEntry>) {
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
