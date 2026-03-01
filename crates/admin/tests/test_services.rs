mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn list_services_empty() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = Request::builder()
        .uri("/admin/services")
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
async fn list_services_with_search_filter() {
    let pool = common::setup_test_db().await;

    // Insert a service directly via SQL for test isolation
    sqlx::query(
        r#"INSERT INTO upstreams (id, name, algorithm) VALUES
        ('a0000000-0000-0000-0000-000000000001', 'svc-searchtest', 'round_robin')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO services (namespace, spec_url, spec_hash, upstream_id, description, tags, status)
        VALUES ('searchtest', 'http://example.com/spec.json', 'abc123',
                'a0000000-0000-0000-0000-000000000001', 'test service', '{}', 'stable')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Search by namespace
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/services?search=searchtest")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);

    // Search with no match
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/services?search=nonexistent")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn list_services_with_status_filter() {
    let pool = common::setup_test_db().await;

    sqlx::query(
        r#"INSERT INTO upstreams (id, name, algorithm) VALUES
        ('b0000000-0000-0000-0000-000000000001', 'svc-statustest', 'round_robin')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO services (namespace, spec_url, spec_hash, upstream_id, description, tags, status)
        VALUES ('statustest', 'http://example.com/spec.json', 'def456',
                'b0000000-0000-0000-0000-000000000001', '', '{}', 'beta')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Filter by beta
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/services?status=beta")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].as_array().unwrap().len() >= 1);

    // Filter by alpha (should not include our beta service)
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .uri("/admin/services?status=alpha")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn get_service() {
    let pool = common::setup_test_db().await;

    sqlx::query(
        r#"INSERT INTO upstreams (id, name, algorithm) VALUES
        ('c0000000-0000-0000-0000-000000000001', 'svc-gettest', 'round_robin')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO services (id, namespace, spec_url, spec_hash, upstream_id, description, tags, status)
        VALUES ('c0000000-0000-0000-0000-000000000099', 'gettest', 'http://example.com/spec.json', 'ghi789',
                'c0000000-0000-0000-0000-000000000001', 'get test', '{}', 'stable')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = common::build_test_app(pool);
    let req = Request::builder()
        .uri("/admin/services/c0000000-0000-0000-0000-000000000099")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["namespace"], "gettest");
}

#[tokio::test]
async fn update_service_metadata() {
    let pool = common::setup_test_db().await;

    sqlx::query(
        r#"INSERT INTO upstreams (id, name, algorithm) VALUES
        ('d0000000-0000-0000-0000-000000000001', 'svc-updatetest', 'round_robin')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO services (id, namespace, spec_url, spec_hash, upstream_id, description, tags, status)
        VALUES ('d0000000-0000-0000-0000-000000000099', 'updatetest', 'http://example.com/spec.json', 'jkl012',
                'd0000000-0000-0000-0000-000000000001', 'original desc', '{"v1"}', 'stable')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({
        "description": "updated desc",
        "tags": ["v2", "production"],
        "status": "deprecated"
    });
    let req = Request::builder()
        .method("PUT")
        .uri("/admin/services/d0000000-0000-0000-0000-000000000099")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["description"], "updated desc");
    assert_eq!(json["status"], "deprecated");
}

#[tokio::test]
async fn update_service_invalid_status_returns_400() {
    let pool = common::setup_test_db().await;

    sqlx::query(
        r#"INSERT INTO upstreams (id, name, algorithm) VALUES
        ('e0000000-0000-0000-0000-000000000001', 'svc-invalidstatus', 'round_robin')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO services (id, namespace, spec_url, spec_hash, upstream_id, description, tags, status)
        VALUES ('e0000000-0000-0000-0000-000000000099', 'invalidstatus', 'http://example.com/spec.json', 'mno345',
                'e0000000-0000-0000-0000-000000000001', '', '{}', 'stable')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = common::build_test_app(pool);
    let payload = serde_json::json!({"status": "invalid-status"});
    let req = Request::builder()
        .method("PUT")
        .uri("/admin/services/e0000000-0000-0000-0000-000000000099")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_service_cascade() {
    let pool = common::setup_test_db().await;

    // Create upstream
    sqlx::query(
        r#"INSERT INTO upstreams (id, name, algorithm) VALUES
        ('f0000000-0000-0000-0000-000000000001', 'svc-deletetest', 'round_robin')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create route
    sqlx::query(
        r#"INSERT INTO routes (id, name, path_prefix, upstream_id, strip_prefix)
        VALUES ('f0000000-0000-0000-0000-000000000002', 'svc-deletetest', '/deletetest',
                'f0000000-0000-0000-0000-000000000001', true)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Create service
    sqlx::query(
        r#"INSERT INTO services (id, namespace, spec_url, spec_hash, upstream_id, route_id, description, tags, status)
        VALUES ('f0000000-0000-0000-0000-000000000099', 'deletetest', 'http://example.com/spec.json', 'pqr678',
                'f0000000-0000-0000-0000-000000000001', 'f0000000-0000-0000-0000-000000000002',
                '', '{}', 'stable')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // Delete service
    let app = common::build_test_app(pool.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri("/admin/services/f0000000-0000-0000-0000-000000000099")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify upstream was also deleted
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM upstreams WHERE id = 'f0000000-0000-0000-0000-000000000001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count.0, 0);
}
