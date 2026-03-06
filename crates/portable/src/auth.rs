use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use subtle::ConstantTimeEq;

pub async fn admin_token_middleware(
    req: Request,
    next: Next,
) -> Response {
    if req.uri().path() == "/admin/health" {
        return next.run(req).await;
    }

    let admin_token = match std::env::var("ADMIN_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "Admin API is disabled — ADMIN_TOKEN is not configured",
                    "code": "AUTH_NOT_CONFIGURED"
                })),
            )
                .into_response();
        }
    };

    let provided = req
        .headers()
        .get("X-Admin-Token")
        .and_then(|v| v.to_str().ok());

    match provided {
        Some(token) if constant_time_eq_str(token, &admin_token) => next.run(req).await,
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

fn constant_time_eq_str(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}
