mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn create_upstream_and_route(pool: &sqlx::PgPool) -> String {
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/upstreams")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"name":"hr-upstream"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let upstream_id = json["id"].as_str().unwrap().to_string();

    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "name": "hr-route",
        "path_prefix": "/hr",
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
async fn create_header_rule_set() {
    let pool = common::setup_test_db().await;
    let route_id = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "X-Custom",
        "header_value": "hello"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["action"], "set");
    assert_eq!(json["header_name"], "X-Custom");
}

#[tokio::test]
async fn create_header_rule_remove() {
    let pool = common::setup_test_db().await;
    let route_id = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "action": "remove",
        "header_name": "X-Remove-Me"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_header_rule_invalid_phase_returns_400() {
    let pool = common::setup_test_db().await;
    let route_id = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "phase": "invalid",
        "action": "set",
        "header_name": "X-Test",
        "header_value": "val"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_header_rule_missing_value_returns_400() {
    let pool = common::setup_test_db().await;
    let route_id = create_upstream_and_route(&pool).await;

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "X-Test"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_and_delete_header_rules() {
    let pool = common::setup_test_db().await;
    let route_id = create_upstream_and_route(&pool).await;

    // Create a rule
    let app = common::build_test_app(pool.clone());
    let payload = serde_json::json!({
        "action": "set",
        "header_name": "X-List",
        "header_value": "val"
    });
    let req = common::authed_request("POST", &format!("/admin/routes/{}/header-rules", route_id))
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let rule_id = created["id"].as_str().unwrap();

    // List
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get(&format!("/admin/routes/{}/header-rules", route_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().unwrap().len() >= 1);

    // Delete
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("DELETE", &format!("/admin/header-rules/{}", rule_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
