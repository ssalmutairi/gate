mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn create_upstream_returns_201() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"test-upstream"}"#))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "test-upstream");
    assert_eq!(json["algorithm"], "round_robin");
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn create_upstream_empty_name_returns_400() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":""}"#))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_upstreams() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_get("/admin/upstreams")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].is_array());
    assert!(json["total"].is_number());
}

#[tokio::test]
async fn get_upstream_not_found() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_get("/admin/upstreams/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_get_update_delete_upstream() {
    let pool = common::setup_test_db().await;

    // Create
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"crud-test"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    // Get
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get(&format!("/admin/upstreams/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Update
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("PUT", &format!("/admin/upstreams/{}", id))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"updated-name"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["name"], "updated-name");

    // Delete
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("DELETE", &format!("/admin/upstreams/{}", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_upstream_not_found() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_request("DELETE", "/admin/upstreams/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_target_not_found() {
    let pool = common::setup_test_db().await;

    // Create upstream first
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"target-del-test"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap();

    // Try to delete non-existent target
    let app = common::build_test_app(pool);
    let req = common::authed_request(
        "DELETE",
        &format!("/admin/upstreams/{}/targets/00000000-0000-0000-0000-000000000000", upstream_id),
    )
    .body(Body::empty())
    .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_upstreams_pagination() {
    let pool = common::setup_test_db().await;

    // Create two upstreams
    for name in &["page-upstream-a", "page-upstream-b"] {
        let app = common::build_test_app(pool.clone());
        let payload = serde_json::json!({"name": name});
        let req = common::authed_request("POST", "/admin/upstreams")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // Page 1, limit 1
    let app = common::build_test_app(pool);
    let req = common::authed_get("/admin/upstreams?page=1&limit=1")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
    assert!(json["total"].as_i64().unwrap() >= 2);
}

#[tokio::test]
async fn add_target_invalid_port() {
    let pool = common::setup_test_db().await;

    // Create upstream
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"port-test"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap();

    // Add target with invalid port
    let app = common::build_test_app(pool);
    let payload = serde_json::json!({"host": "example.com", "port": 0});
    let req = common::authed_request("POST", &format!("/admin/upstreams/{}/targets", upstream_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
