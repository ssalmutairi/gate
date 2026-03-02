use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Conflict(String),
    Validation(String),
    Unauthorized,
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Invalid or missing admin token".to_string(),
            ),
            AppError::Internal(msg) => {
                tracing::error!(error = %msg, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "Internal server error".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message, "code": code }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("Resource not found".to_string()),
            sqlx::Error::Database(db_err) => {
                if db_err.is_unique_violation() {
                    AppError::Conflict("Resource already exists".to_string())
                } else if db_err.is_foreign_key_violation() {
                    AppError::Validation("Referenced resource does not exist".to_string())
                } else {
                    AppError::Internal(db_err.to_string())
                }
            }
            _ => AppError::Internal(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    async fn response_to_json(resp: axum::response::Response) -> (StatusCode, serde_json::Value) {
        let status = resp.status();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let err = AppError::NotFound("thing not found".into());
        let (status, json) = response_to_json(err.into_response()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["code"], "NOT_FOUND");
        assert_eq!(json["error"], "thing not found");
    }

    #[tokio::test]
    async fn conflict_returns_409() {
        let err = AppError::Conflict("already exists".into());
        let (status, json) = response_to_json(err.into_response()).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(json["code"], "CONFLICT");
        assert_eq!(json["error"], "already exists");
    }

    #[tokio::test]
    async fn validation_returns_400() {
        let err = AppError::Validation("bad input".into());
        let (status, json) = response_to_json(err.into_response()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["code"], "VALIDATION_ERROR");
        assert_eq!(json["error"], "bad input");
    }

    #[tokio::test]
    async fn unauthorized_returns_401() {
        let err = AppError::Unauthorized;
        let (status, json) = response_to_json(err.into_response()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn internal_returns_500_and_hides_message() {
        let err = AppError::Internal("sensitive db error".into());
        let (status, json) = response_to_json(err.into_response()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["code"], "INTERNAL_ERROR");
        // Must NOT expose the original message
        assert_eq!(json["error"], "Internal server error");
        assert!(!json["error"].as_str().unwrap().contains("sensitive"));
    }

    #[test]
    fn from_sqlx_row_not_found() {
        let err: AppError = sqlx::Error::RowNotFound.into();
        match err {
            AppError::NotFound(msg) => assert_eq!(msg, "Resource not found"),
            other => panic!("expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn from_sqlx_generic_error() {
        let err: AppError = sqlx::Error::PoolTimedOut.into();
        match err {
            AppError::Internal(_) => {}
            other => panic!("expected Internal, got {:?}", other),
        }
    }
}
