use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub async fn admin_token_middleware(
    req: Request,
    next: Next,
) -> Response {
    // Skip auth for health endpoint
    if req.uri().path() == "/admin/health" {
        return next.run(req).await;
    }

    let admin_token = match std::env::var("ADMIN_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => return next.run(req).await, // No token configured, skip auth
    };

    let provided = req
        .headers()
        .get("X-Admin-Token")
        .and_then(|v| v.to_str().ok());

    match provided {
        Some(token) if token == admin_token => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Invalid or missing admin token",
                "code": "UNAUTHORIZED"
            })),
        )
            .into_response(),
    }
}
