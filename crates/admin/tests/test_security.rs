mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

// --- Host validation on targets ---

#[tokio::test]
async fn target_host_with_slash_rejected() {
    let pool = common::setup_test_db().await;

    // Create upstream first
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"sec-test"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap();

    // Try to add target with slash in host
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({"host": "evil.com/path", "port": 80});
    let req = common::authed_request("POST", &format!("/admin/upstreams/{}/targets", upstream_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn target_host_with_at_sign_rejected() {
    let pool = common::setup_test_db().await;

    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"sec-at"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap();

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({"host": "user@evil.com", "port": 80});
    let req = common::authed_request("POST", &format!("/admin/upstreams/{}/targets", upstream_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn target_host_with_space_rejected() {
    let pool = common::setup_test_db().await;

    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"sec-space"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap();

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({"host": "evil .com", "port": 80});
    let req = common::authed_request("POST", &format!("/admin/upstreams/{}/targets", upstream_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn target_valid_host_accepted() {
    let pool = common::setup_test_db().await;

    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"sec-valid"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap();

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({"host": "api.example.com", "port": 443, "tls": true});
    let req = common::authed_request("POST", &format!("/admin/upstreams/{}/targets", upstream_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn upstream_name_too_long_rejected() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let long_name = "x".repeat(256);
    let payload = serde_json::json!({"name": long_name});
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// --- Header rule CRLF injection ---

async fn create_test_route(pool: &sqlx::PgPool) -> String {
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"sec-hr-up"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap().to_string();

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "name": "sec-hr-route",
        "path_prefix": "/sec-hr",
        "upstream_id": upstream_id
    });
    let req = common::authed_request("POST", "/admin/routes")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn header_rule_crlf_in_value_rejected() {
    let pool = common::setup_test_db().await;
    let route_id = create_test_route(&pool).await;

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "X-Injected",
        "header_value": "value\r\nX-Evil: injected"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("CR or LF"));
}

#[tokio::test]
async fn header_rule_newline_only_rejected() {
    let pool = common::setup_test_db().await;
    let route_id = create_test_route(&pool).await;

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "X-Test",
        "header_value": "line1\nline2"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn header_rule_invalid_header_name_rejected() {
    let pool = common::setup_test_db().await;
    let route_id = create_test_route(&pool).await;

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "Invalid Header Name With Spaces",
        "header_value": "value"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn header_rule_value_too_long_rejected() {
    let pool = common::setup_test_db().await;
    let route_id = create_test_route(&pool).await;

    let app = common::build_test_app(pool.clone());
    let long_value = "x".repeat(8193);
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "X-Long",
        "header_value": long_value
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// --- Service input length limits ---

#[tokio::test]
async fn service_import_namespace_too_long_rejected() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let long_ns = "x".repeat(256);
    let payload = serde_json::json!({
        "namespace": long_ns,
        "spec_content": r#"{"openapi":"3.0.0","info":{"title":"test","version":"1"},"servers":[{"url":"http://example.com"}],"paths":{}}"#
    });
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn service_import_url_too_long_rejected() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let long_url = format!("https://example.com/{}", "x".repeat(2049));
    let payload = serde_json::json!({
        "namespace": "test-ns",
        "url": long_url
    });
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// --- Auth fail-closed (verify via non-health endpoint) ---

#[tokio::test]
async fn unauthenticated_request_returns_401() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    // Request without X-Admin-Token header
    let req = axum::http::Request::builder()
        .uri("/admin/upstreams")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_token_returns_401() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = axum::http::Request::builder()
        .uri("/admin/upstreams")
        .header("X-Admin-Token", "wrong-token-value")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
