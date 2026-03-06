use axum::body::Body;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use std::sync::LazyLock;

static PROXY_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

pub async fn proxy_to_gateway(req: Request) -> Response {
    let proxy_port: u16 = std::env::var("PROXY_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let path = req.uri().path().strip_prefix("/gateway").unwrap_or(req.uri().path());
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let url = format!("http://127.0.0.1:{proxy_port}{path}{query}");

    let method = req.method().clone();
    let headers = req.headers().clone();

    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    let mut upstream_req = PROXY_CLIENT.request(method, &url);
    for (name, value) in headers.iter() {
        if name == "host" || name == "connection" || name == "transfer-encoding"
            || name == "accept-encoding"
        {
            continue;
        }
        upstream_req = upstream_req.header(name.as_str(), value);
    }
    if !body_bytes.is_empty() {
        upstream_req = upstream_req.body(body_bytes.to_vec());
    }

    match upstream_req.send().await {
        Ok(upstream_res) => {
            let status = StatusCode::from_u16(upstream_res.status().as_u16())
                .unwrap_or(StatusCode::BAD_GATEWAY);
            let mut builder = Response::builder().status(status);
            for (name, value) in upstream_res.headers() {
                if name == "transfer-encoding" || name == "connection" {
                    continue;
                }
                builder = builder.header(name.as_str(), value);
            }
            let body = upstream_res.bytes().await.unwrap_or_default();
            builder.body(Body::from(body)).unwrap_or_else(|_| {
                (StatusCode::BAD_GATEWAY, "Failed to build response").into_response()
            })
        }
        Err(e) => {
            tracing::error!(error = %e, url = %url, "Gateway proxy error");
            (StatusCode::BAD_GATEWAY, format!("Proxy error: {e}")).into_response()
        }
    }
}
