use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../dashboard/dist"]
struct DashboardAssets;

pub async fn dashboard_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if let Some(file) = DashboardAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let mut response = (StatusCode::OK, file.data).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, mime.as_ref().parse().unwrap());

        if path.starts_with("assets/") {
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".parse().unwrap(),
            );
        }

        return response;
    }

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
