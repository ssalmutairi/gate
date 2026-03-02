mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn create_upstream_and_route(pool: &sqlx::PgPool) -> (String, String) {
    // Create upstream
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"rl-upstream"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap().to_string();

    // Create route
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "name": "rl-route",
        "path_prefix": "/rl",
        "upstream_id": upstream_id
    });
    let req = common::authed_request("POST", "/admin/routes")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let route_id = json["id"].as_str().unwrap().to_string();

    (upstream_id, route_id)
}

#[tokio::test]
async fn create_rate_limit_returns_201() {
    let pool = common::setup_test_db().await;
    let (_upstream_id, route_id) = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "route_id": route_id,
        "requests_per_second": 100
    });
    let req = common::authed_request("POST", "/admin/rate-limits")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["requests_per_second"], 100);
}

#[tokio::test]
async fn create_rate_limit_invalid_rps_returns_400() {
    let pool = common::setup_test_db().await;
    let (_upstream_id, route_id) = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "route_id": route_id,
        "requests_per_second": -1
    });
    let req = common::authed_request("POST", "/admin/rate-limits")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_and_delete_rate_limit() {
    let pool = common::setup_test_db().await;
    let (_upstream_id, route_id) = create_upstream_and_route(&pool).await;

    // Create
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "route_id": route_id,
        "requests_per_second": 50
    });
    let req = common::authed_request("POST", "/admin/rate-limits")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = json["id"].as_str().unwrap();

    // Update
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({"requests_per_second": 200});
    let req = common::authed_request("PUT", &format!("/admin/rate-limits/{}", id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["requests_per_second"], 200);

    // Delete
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("DELETE", &format!("/admin/rate-limits/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn list_rate_limits_empty() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_get("/admin/rate-limits")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn list_rate_limits_pagination() {
    let pool = common::setup_test_db().await;
    let (_upstream_id, route_id) = create_upstream_and_route(&pool).await;

    // Create two rate limits (need two routes since rate_limits has unique constraint on route_id)
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "route_id": route_id,
        "requests_per_second": 100
    });
    let req = common::authed_request("POST", "/admin/rate-limits")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // List with page/limit
    let app = common::build_test_app(pool);
    let req = common::authed_get("/admin/rate-limits?page=1&limit=1")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
    assert!(json["total"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn create_rate_limit_invalid_route_id() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let payload = serde_json::json!({
        "route_id": "00000000-0000-0000-0000-000000000000",
        "requests_per_second": 100
    });
    let req = common::authed_request("POST", "/admin/rate-limits")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_rate_limit_invalid_limit_by() {
    let pool = common::setup_test_db().await;
    let (_upstream_id, route_id) = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "route_id": route_id,
        "requests_per_second": 100,
        "limit_by": "invalid_value"
    });
    let req = common::authed_request("POST", "/admin/rate-limits")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_nonexistent_rate_limit_returns_404() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_request("DELETE", "/admin/rate-limits/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_nonexistent_rate_limit_returns_404() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let payload = serde_json::json!({"requests_per_second": 200});
    let req = common::authed_request("PUT", "/admin/rate-limits/00000000-0000-0000-0000-000000000000")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
