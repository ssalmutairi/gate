mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn health_no_auth_required() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = Request::builder()
        .uri("/admin/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

/// Tests that depend on ADMIN_TOKEN env var must run sequentially in one test
/// to avoid races from parallel test execution.
#[tokio::test]
async fn auth_token_scenarios() {
    let pool = common::setup_test_db().await;

    // Scenario 1: No token configured → access forbidden (fail-closed)
    std::env::remove_var("ADMIN_TOKEN");
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/upstreams")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "no token configured should reject access");

    // Scenario 2: Token configured, valid token → access allowed
    std::env::set_var("ADMIN_TOKEN", "test-secret");
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/upstreams")
        .header("X-Admin-Token", "test-secret")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "valid token should pass");

    // Scenario 3: Token configured, missing token → 401
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/upstreams")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "missing token should return 401");

    // Scenario 4: Token configured, wrong token → 401
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/upstreams")
        .header("X-Admin-Token", "wrong-token")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "wrong token should return 401");

    // Restore test token for other tests
    std::env::set_var("ADMIN_TOKEN", common::TEST_ADMIN_TOKEN);
}
