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
    /// Upstream response body snippet for error responses (status >= 400), capped at 4 KB.
    pub error_body: Option<String>,
    /// Microseconds since UNIX epoch when the request completed.
    pub timestamp_us: u64,
}

/// Maximum number of pending log entries before new entries are dropped.
const LOG_CHANNEL_CAPACITY: usize = 10_000;

/// Maximum entries per flush batch.
const BATCH_SIZE: usize = 100;

/// How often to flush buffered entries.
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Elastic APM connection configuration.
pub struct ElasticApmConfig {
    pub url: String,
    pub token: Option<String>,
}

/// Selects the logging backend for request logs.
pub enum LogBackend {
    Postgres { database_url: String },
    ElasticApm(ElasticApmConfig),
}

/// Generates a batch writer loop that collects entries from `$rx` and flushes via `$flush_expr`.
/// Eliminates duplication between PG and Elastic backends.
macro_rules! batch_writer_loop {
    ($rx:expr, $buffer:ident, $flush_expr:expr) => {{
        let mut $buffer: Vec<RequestLogEntry> = Vec::with_capacity(BATCH_SIZE);
        loop {
            let deadline = tokio::time::sleep(FLUSH_INTERVAL);
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    entry = $rx.recv() => {
                        match entry {
                            Some(e) => {
                                $buffer.push(e);
                                if $buffer.len() >= BATCH_SIZE {
                                    break;
                                }
                            }
                            None => {
                                if !$buffer.is_empty() {
                                    $flush_expr;
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
            if !$buffer.is_empty() {
                $flush_expr;
            }
        }
    }};
}

/// Creates a log sender channel and spawns the background batch writer.
/// Returns the sender that can be used to enqueue log entries.
pub fn spawn_log_writer(backend: LogBackend) -> mpsc::Sender<RequestLogEntry> {
    let (tx, rx) = mpsc::channel(LOG_CHANNEL_CAPACITY);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build log writer runtime");

        rt.block_on(async move {
            match backend {
                LogBackend::Postgres { database_url } => {
                    let pool = sqlx::postgres::PgPoolOptions::new()
                        .max_connections(3)
                        .connect(&database_url)
                        .await
                        .expect("Log writer failed to connect to database");

                    batch_writer_pg(pool, rx).await;
                }
                LogBackend::ElasticApm(config) => {
                    let client = reqwest::Client::new();
                    batch_writer_elastic(client, config, rx).await;
                }
            }
        });
    });

    tx
}

// ---------------------------------------------------------------------------
// PostgreSQL backend
// ---------------------------------------------------------------------------

/// Background task that batches log entries and inserts them periodically.
pub(crate) async fn batch_writer_pg(pool: PgPool, mut rx: mpsc::Receiver<RequestLogEntry>) {
    batch_writer_loop!(rx, buffer, flush_batch_pg(&pool, &mut buffer).await);
}

/// Flush a batch of log entries into the database.
pub(crate) async fn flush_batch_pg(pool: &PgPool, buffer: &mut Vec<RequestLogEntry>) {
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

// ---------------------------------------------------------------------------
// Elastic APM backend
// ---------------------------------------------------------------------------

/// Background task that batches log entries and sends them to Elastic APM.
pub(crate) async fn batch_writer_elastic(
    client: reqwest::Client,
    config: ElasticApmConfig,
    mut rx: mpsc::Receiver<RequestLogEntry>,
) {
    batch_writer_loop!(rx, buffer, { flush_elastic(&client, &config, &mut buffer).await; });
}

/// Flush a batch of log entries to the Elastic APM Intake V2 API.
/// Returns `true` if the APM server accepted the batch (HTTP 2xx).
pub(crate) async fn flush_elastic(
    client: &reqwest::Client,
    config: &ElasticApmConfig,
    buffer: &mut Vec<RequestLogEntry>,
) -> bool {
    let version = env!("CARGO_PKG_VERSION");
    let metadata = serde_json::json!({
        "metadata": {
            "service": {
                "name": "gate-proxy",
                "agent": { "name": "gate", "version": version }
            }
        }
    });

    // Pre-allocate ~1 KB per entry (transaction + possible error event)
    let mut ndjson = String::with_capacity(buffer.len() * 1024 + 256);
    ndjson.push_str(&serde_json::to_string(&metadata).expect("metadata serialization"));
    ndjson.push('\n');

    for entry in buffer.iter() {
        let trace_id = Uuid::new_v4().simple().to_string();
        let tx_id = &trace_id[..16];

        let outcome = if entry.status_code < 400 { "success" } else { "failure" };

        let context = serde_json::json!({
            "request": {
                "method": entry.method,
                "url": { "pathname": entry.path }
            },
            "tags": {
                "client_ip": entry.client_ip,
                "upstream_target": entry.upstream_target,
                "route_id": entry.route_id.map(|id| id.to_string()),
            }
        });

        let transaction = serde_json::json!({
            "transaction": {
                "id": tx_id,
                "trace_id": trace_id,
                "name": format!("{} {}", entry.method, entry.path),
                "type": "request",
                "timestamp": entry.timestamp_us,
                "duration": entry.latency_ms,
                "result": entry.status_code.to_string(),
                "outcome": outcome,
                "context": context,
                "span_count": { "started": 0 }
            }
        });

        ndjson.push_str(&serde_json::to_string(&transaction).expect("transaction serialization"));
        ndjson.push('\n');

        // Emit an APM error event for failed requests so they appear in the Errors tab
        if entry.status_code >= 400 {
            let error_id = Uuid::new_v4().simple().to_string();
            let error_message = match entry.error_body {
                Some(ref err_body) => {
                    let clean = strip_html_tags(err_body);
                    format!("HTTP {} — {}", entry.status_code, clean)
                }
                None => format!("HTTP {}", entry.status_code),
            };
            let error = serde_json::json!({
                "error": {
                    "id": &error_id[..16],
                    "trace_id": trace_id,
                    "transaction_id": tx_id,
                    "parent_id": tx_id,
                    "timestamp": entry.timestamp_us,
                    "culprit": format!("{} {}", entry.method, entry.path),
                    "exception": {
                        "message": error_message,
                        "type": format!("HTTP {}xx", entry.status_code / 100),
                    },
                    "context": context
                }
            });
            ndjson.push_str(&serde_json::to_string(&error).expect("error serialization"));
            ndjson.push('\n');
        }
    }

    let url = format!("{}/intake/v2/events", config.url.trim_end_matches('/'));
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson);

    if let Some(ref token) = config.token {
        req = req.bearer_auth(token);
    }

    let accepted = match req.send().await {
        Ok(resp) if !resp.status().is_success() => {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            tracing::error!(
                count = buffer.len(),
                status = %status,
                body = %resp_body,
                "Elastic APM intake rejected batch"
            );
            false
        }
        Err(e) => {
            tracing::error!(count = buffer.len(), error = %e, "Failed to send logs to Elastic APM");
            false
        }
        Ok(_) => {
            tracing::debug!(count = buffer.len(), "Flushed request logs to Elastic APM");
            true
        }
    };

    buffer.clear();
    accepted
}

/// Strip HTML tags and style/script block contents to produce a readable plain-text error.
/// Collapses whitespace runs and trims the result.
fn strip_html_tags(input: &str) -> String {
    // Lowercase once for case-insensitive searching
    let lower = input.to_lowercase();
    let bytes = input.as_bytes();
    let lower_bytes = lower.as_bytes();

    // Bitmap of bytes to keep (true = keep, false = strip)
    let mut keep = vec![true; bytes.len()];

    // Mark <style>...</style> and <script>...</script> blocks for removal
    for tag in &[b"style".as_slice(), b"script".as_slice()] {
        let open_prefix = {
            let mut v = vec![b'<'];
            v.extend_from_slice(tag);
            v
        };
        let close_tag = {
            let mut v = vec![b'<', b'/'];
            v.extend_from_slice(tag);
            v.push(b'>');
            v
        };

        let mut pos = 0;
        while pos < lower_bytes.len() {
            if let Some(offset) = find_bytes(&lower_bytes[pos..], &open_prefix) {
                let start = pos + offset;
                if let Some(end_offset) = find_bytes(&lower_bytes[start..], &close_tag) {
                    let end = start + end_offset + close_tag.len();
                    for b in &mut keep[start..end] {
                        *b = false;
                    }
                    pos = end;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    // Mark HTML comments for removal
    {
        let mut pos = 0;
        while pos < lower_bytes.len() {
            if let Some(offset) = find_bytes(&lower_bytes[pos..], b"<!--") {
                let start = pos + offset;
                if let Some(end_offset) = find_bytes(&lower_bytes[start..], b"-->") {
                    let end = start + end_offset + 3;
                    for b in &mut keep[start..end] {
                        *b = false;
                    }
                    pos = end;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    // Strip remaining HTML tags and collect text
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for (i, &byte) in bytes.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        match byte {
            b'<' => in_tag = true,
            b'>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(byte as char),
            _ => {}
        }
    }

    // Collapse whitespace runs into single spaces
    let mut result = String::with_capacity(out.len());
    let mut prev_space = true;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

/// Find a byte pattern in a byte slice. Returns the offset of the first occurrence.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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

    fn now_us() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
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
            error_body: if status >= 400 {
                Some(format!("{{\"error\": \"test error for status {}\"}}", status))
            } else {
                None
            },
            timestamp_us: now_us(),
        }
    }

    #[tokio::test]
    async fn flush_batch_inserts_rows() {
        let _lock = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let pool = setup_test_pool().await;
        let mut buffer = vec![make_entry("GET", "/single-insert", 200)];
        flush_batch_pg(&pool, &mut buffer).await;
        assert!(buffer.is_empty());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE path = '/single-insert'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn flush_batch_multiple_entries() {
        let _lock = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let pool = setup_test_pool().await;
        let mut buffer = vec![
            make_entry("GET", "/multi-a", 200),
            make_entry("POST", "/multi-b", 201),
            make_entry("DELETE", "/multi-c", 204),
        ];
        flush_batch_pg(&pool, &mut buffer).await;
        assert!(buffer.is_empty());

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE path LIKE '/multi-%'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 3);
    }

    #[tokio::test]
    async fn batch_writer_flushes_on_channel_close() {
        let _lock = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let pool = setup_test_pool().await;
        let (tx, rx) = mpsc::channel(100);

        // Send some entries then drop the sender
        tx.send(make_entry("GET", "/batch-close", 200)).await.unwrap();
        tx.send(make_entry("POST", "/batch-close", 201)).await.unwrap();
        drop(tx);

        // batch_writer should process remaining entries and return
        batch_writer_pg(pool.clone(), rx).await;

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM request_logs WHERE path = '/batch-close'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn batch_writer_empty_channel_exits() {
        let _lock = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let pool = setup_test_pool().await;
        let (_tx, rx) = mpsc::channel::<RequestLogEntry>(100);

        // Drop sender immediately — batch_writer should exit cleanly
        drop(_tx);
        batch_writer_pg(pool, rx).await;
        // If we get here, it exited properly
    }

    #[tokio::test]
    async fn flush_elastic_sends_to_apm() {
        let apm_url = std::env::var("ELASTIC_APM_URL")
            .unwrap_or_else(|_| "http://localhost:8200".to_string());

        // Quick connectivity check — skip if APM is not running
        let client = reqwest::Client::new();
        if client.get(&apm_url).send().await.is_err() {
            eprintln!("Skipping elastic APM test — server not reachable at {apm_url}");
            return;
        }

        let config = ElasticApmConfig {
            url: apm_url,
            token: None,
        };

        let mut buffer = vec![
            make_entry("GET", "/elastic-test", 200),
            make_entry("POST", "/elastic-test", 201),
            make_entry("DELETE", "/elastic-test", 500),
        ];

        let accepted = flush_elastic(&client, &config, &mut buffer).await;
        assert!(accepted, "Elastic APM should accept the batch with HTTP 202");
        assert!(buffer.is_empty(), "Buffer should be cleared after flush");
    }

    #[tokio::test]
    async fn batch_writer_elastic_flushes_on_channel_close() {
        let apm_url = std::env::var("ELASTIC_APM_URL")
            .unwrap_or_else(|_| "http://localhost:8200".to_string());

        let client = reqwest::Client::new();
        if client.get(&apm_url).send().await.is_err() {
            eprintln!("Skipping elastic APM test — server not reachable at {apm_url}");
            return;
        }

        let config = ElasticApmConfig {
            url: apm_url,
            token: None,
        };

        let (tx, rx) = mpsc::channel(100);
        tx.send(make_entry("GET", "/elastic-batch-close", 200)).await.unwrap();
        tx.send(make_entry("POST", "/elastic-batch-close", 201)).await.unwrap();
        drop(tx);

        // Should process remaining entries and return without panic
        batch_writer_elastic(client, config, rx).await;
    }

    #[test]
    fn strip_html_tags_basic() {
        assert_eq!(strip_html_tags("<p>Hello</p>"), "Hello");
    }

    #[test]
    fn strip_html_tags_removes_style_blocks() {
        let input = "<style>body{color:red}</style><p>Text</p>";
        assert_eq!(strip_html_tags(input), "Text");
    }

    #[test]
    fn strip_html_tags_removes_script_blocks() {
        let input = "<script>alert('xss')</script><p>Safe</p>";
        assert_eq!(strip_html_tags(input), "Safe");
    }

    #[test]
    fn strip_html_tags_removes_comments() {
        let input = "<!-- comment --><p>Visible</p>";
        assert_eq!(strip_html_tags(input), "Visible");
    }

    #[test]
    fn strip_html_tags_collapses_whitespace() {
        let input = "<p>  lots   of    spaces  </p>";
        assert_eq!(strip_html_tags(input), "lots of spaces");
    }

    #[test]
    fn strip_html_tags_plain_text() {
        assert_eq!(strip_html_tags("no tags here"), "no tags here");
    }
}
