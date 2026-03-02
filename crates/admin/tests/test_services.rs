mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn list_services_empty() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_get("/admin/services")
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
    let req = common::authed_get("/admin/services?search=searchtest")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);

    // Search with no match
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get("/admin/services?search=nonexistent")
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
    let req = common::authed_get("/admin/services?status=beta")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["data"].as_array().unwrap().len() >= 1);

    // Filter by alpha (should not include our beta service)
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get("/admin/services?status=alpha")
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
    let req = common::authed_get("/admin/services/c0000000-0000-0000-0000-000000000099")
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
    let req = common::authed_request("PUT", "/admin/services/d0000000-0000-0000-0000-000000000099")
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
    let req = common::authed_request("PUT", "/admin/services/e0000000-0000-0000-0000-000000000099")
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
    let req = common::authed_request("DELETE", "/admin/services/f0000000-0000-0000-0000-000000000099")
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

#[tokio::test]
async fn import_service_with_inline_spec() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let spec = json!({
        "openapi": "3.0.0",
        "info": {"title": "Test API", "version": "1.0"},
        "servers": [{"url": "https://api.example.com/v1"}],
        "paths": {}
    });

    let payload = json!({
        "namespace": "inline-test",
        "spec_content": spec.to_string()
    });

    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["namespace"], "inline-test");
    assert_eq!(json["version"], 1);
}

#[tokio::test]
async fn reimport_same_hash_returns_409() {
    let pool = common::setup_test_db().await;

    let spec = json!({
        "openapi": "3.0.0",
        "info": {"title": "Dup API", "version": "1.0"},
        "servers": [{"url": "https://api.example.com/v1"}],
        "paths": {}
    });

    let payload = json!({
        "namespace": "dup-hash",
        "spec_content": spec.to_string()
    });

    // First import — should succeed
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Second import with same spec — should 409
    let app = common::build_test_app(pool);
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn reimport_different_hash_bumps_version() {
    let pool = common::setup_test_db().await;

    let spec_v1 = json!({
        "openapi": "3.0.0",
        "info": {"title": "Version API", "version": "1.0"},
        "servers": [{"url": "https://api.example.com/v1"}],
        "paths": {}
    });

    let payload_v1 = json!({
        "namespace": "version-bump",
        "spec_content": spec_v1.to_string()
    });

    // First import
    let app = common::build_test_app(pool.clone());
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload_v1.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Second import with different spec
    let spec_v2 = json!({
        "openapi": "3.0.0",
        "info": {"title": "Version API", "version": "2.0"},
        "servers": [{"url": "https://api.example.com/v2"}],
        "paths": {"/users": {"get": {"summary": "List users"}}}
    });

    let payload_v2 = json!({
        "namespace": "version-bump",
        "spec_content": spec_v2.to_string()
    });

    let app = common::build_test_app(pool);
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload_v2.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["version"], 2);
}

#[tokio::test]
async fn missing_url_and_content_returns_400() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let payload = json!({"namespace": "no-spec"});
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_json_spec_returns_400() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let payload = json!({
        "namespace": "bad-json",
        "spec_content": "not valid json {{"
    });
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn no_servers_in_spec_returns_400() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let spec = json!({
        "openapi": "3.0.0",
        "info": {"title": "No Servers", "version": "1.0"},
        "paths": {}
    });

    let payload = json!({
        "namespace": "no-servers",
        "spec_content": spec.to_string()
    });

    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_service_spec_returns_json() {
    let pool = common::setup_test_db().await;

    let spec = json!({
        "openapi": "3.0.0",
        "info": {"title": "Spec Test", "version": "1.0"},
        "servers": [{"url": "https://api.example.com"}],
        "paths": {}
    });

    // Import a service first
    let app = common::build_test_app(pool.clone());
    let payload = json!({
        "namespace": "spec-fetch",
        "spec_content": spec.to_string()
    });
    let req = common::authed_request("POST", "/admin/services/import")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap();

    // Fetch the spec
    let app = common::build_test_app(pool);
    let req = common::authed_get(&format!("/admin/services/{}/spec", id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let spec_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(spec_json["openapi"], "3.0.0");
}

#[tokio::test]
async fn get_service_not_found_returns_404() {
    let pool = common::setup_test_db().await;
    let app = common::build_test_app(pool);

    let req = common::authed_get("/admin/services/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_services_pagination() {
    let pool = common::setup_test_db().await;

    // Create two services
    for ns in &["page-svc-a", "page-svc-b"] {
        let spec = json!({
            "openapi": "3.0.0",
            "info": {"title": ns, "version": "1.0"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {}
        });
        let payload = json!({
            "namespace": ns,
            "spec_content": spec.to_string()
        });
        let app = common::build_test_app(pool.clone());
        let req = common::authed_request("POST", "/admin/services/import")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_success(), "import failed for {ns}");
    }

    // Page 1, limit 1
    let app = common::build_test_app(pool.clone());
    let req = common::authed_get("/admin/services?page=1&limit=1")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
    assert!(json["total"].as_i64().unwrap() >= 2);
    assert_eq!(json["page"], 1);
    assert_eq!(json["limit"], 1);
}
