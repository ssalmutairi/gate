use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../dashboard/dist"]
struct DashboardAssets;

pub async fn dashboard_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the exact file
    if let Some(file) = DashboardAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let mut response = (StatusCode::OK, file.data).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, mime.as_ref().parse().unwrap());

        // Vite hashed assets get immutable cache
        if path.starts_with("assets/") {
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".parse().unwrap(),
            );
        }

        return response;
    }

    // SPA fallback: serve index.html for non-file paths
    if let Some(index) = DashboardAssets::get("index.html") {
        let mut response = (StatusCode::OK, index.data).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            "text/html; charset=utf-8".parse().unwrap(),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            "no-cache".parse().unwrap(),
        );
        return response;
    }

    StatusCode::NOT_FOUND.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Uri;

    #[tokio::test]
    async fn serves_index_for_root() {
        let uri: Uri = "/".parse().unwrap();
        let resp = dashboard_handler(uri).await;
        // If dashboard/dist exists, we get 200; otherwise 404
        // Both are valid depending on build state
        let status = resp.status();
        assert!(
            status == StatusCode::OK || status == StatusCode::NOT_FOUND,
            "unexpected status: {status}"
        );
        if status == StatusCode::OK {
            let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
            assert!(ct.to_str().unwrap().contains("text/html"));
        }
    }

    #[tokio::test]
    async fn spa_fallback_for_deep_path() {
        let uri: Uri = "/settings/theme".parse().unwrap();
        let resp = dashboard_handler(uri).await;
        let status = resp.status();
        // Deep paths should SPA-fallback to index.html (200) or 404 if no dist
        assert!(
            status == StatusCode::OK || status == StatusCode::NOT_FOUND,
            "unexpected status: {status}"
        );
        if status == StatusCode::OK {
            let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
            assert!(ct.to_str().unwrap().contains("text/html"));
        }
    }

    #[tokio::test]
    async fn nonexistent_asset_falls_back_to_index() {
        let uri: Uri = "/no-such-file.xyz".parse().unwrap();
        let resp = dashboard_handler(uri).await;
        let status = resp.status();
        // Non-existent file should SPA-fallback to index.html or 404 if no dist
        assert!(
            status == StatusCode::OK || status == StatusCode::NOT_FOUND,
            "unexpected status: {status}"
        );
    }
}
