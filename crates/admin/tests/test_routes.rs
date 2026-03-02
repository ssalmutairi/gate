mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn create_upstream(pool: &sqlx::PgPool) -> String {
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"route-test-upstream"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_route_returns_201() {
    let pool = common::setup_test_db().await;
    let upstream_id = create_upstream(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "name": "test-route",
        "path_prefix": "/api",
        "upstream_id": upstream_id
    });
    let req = common::authed_request("POST", "/admin/routes")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "test-route");
    assert_eq!(json["path_prefix"], "/api");
}

#[tokio::test]
async fn create_route_with_max_body_bytes() {
    let pool = common::setup_test_db().await;
    let upstream_id = create_upstream(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "name": "body-limit-route",
        "path_prefix": "/limited",
        "upstream_id": upstream_id,
        "max_body_bytes": 1024
    });
    let req = common::authed_request("POST", "/admin/routes")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["max_body_bytes"], 1024);
}

#[tokio::test]
async fn create_route_invalid_prefix_returns_400() {
    let pool = common::setup_test_db().await;
    let upstream_id = create_upstream(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "name": "bad-route",
        "path_prefix": "no-slash",
        "upstream_id": upstream_id
    });
    let req = common::authed_request("POST", "/admin/routes")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn crud_route() {
    let pool = common::setup_test_db().await;
    let upstream_id = create_upstream(&pool).await;

    // Create
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "name": "crud-route",
        "path_prefix": "/crud",
        "upstream_id": upstream_id
    });
    let req = common::authed_request("POST", "/admin/routes")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    // List
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get("/admin/routes")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Get
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get(&format!("/admin/routes/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Update with max_body_bytes
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({"max_body_bytes": 2048});
    let req = common::authed_request("PUT", &format!("/admin/routes/{}", id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["max_body_bytes"], 2048);

    // Delete
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("DELETE", &format!("/admin/routes/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
