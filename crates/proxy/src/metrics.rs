use once_cell::sync::Lazy;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec,
    CounterVec, Encoder, Gauge, GaugeVec, HistogramVec, TextEncoder,
};

// Request counters
pub static REQUESTS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "gateway_requests_total",
        "Total number of requests",
        &["route", "method", "status"]
    )
    .unwrap()
});

// Request duration histogram
pub static REQUEST_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "gateway_request_duration_seconds",
        "Request duration in seconds",
        &["route", "method"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap()
});

// Upstream errors
pub static UPSTREAM_ERRORS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "gateway_upstream_errors_total",
        "Total upstream errors",
        &["upstream", "target"]
    )
    .unwrap()
});

// Rate limit hits
pub static RATE_LIMIT_HITS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "gateway_rate_limit_hits_total",
        "Total rate limit hits",
        &["route", "limit_by"]
    )
    .unwrap()
});

// Auth failures
pub static AUTH_FAILURES: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "gateway_auth_failures_total",
        "Total authentication failures",
        &["route"]
    )
    .unwrap()
});

// Active connections gauge
pub static ACTIVE_CONNECTIONS: Lazy<Gauge> = Lazy::new(|| {
    prometheus::register_gauge!(
        "gateway_active_connections",
        "Current active connections"
    )
    .unwrap()
});

// Upstream health gauge
pub static UPSTREAM_HEALTH: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "gateway_upstream_health",
        "Upstream target health (1=healthy, 0=unhealthy)",
        &["upstream", "target"]
    )
    .unwrap()
});

/// Initialize all metrics (forces lazy statics to register).
pub fn init() {
    Lazy::force(&REQUESTS_TOTAL);
    Lazy::force(&REQUEST_DURATION);
    Lazy::force(&UPSTREAM_ERRORS);
    Lazy::force(&RATE_LIMIT_HITS);
    Lazy::force(&AUTH_FAILURES);
    Lazy::force(&ACTIVE_CONNECTIONS);
    Lazy::force(&UPSTREAM_HEALTH);
}

/// Encode all metrics as Prometheus text format.
pub fn encode_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

/// Start a lightweight HTTP server that serves /metrics on the given port.
pub fn spawn_metrics_server(port: u16) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build metrics runtime");

        rt.block_on(async move {
            use tokio::io::AsyncWriteExt;
            let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
                .await
                .expect("Failed to bind metrics server");

            tracing::info!(port = port, "Metrics server listening");

            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    continue;
                };

                tokio::spawn(async move {
                    let body = encode_metrics();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
    });
}
