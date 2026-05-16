use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("too many requests")]
    RateLimited { retry_after: u64 },
    #[error("validation: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(&'static str),
    #[error("internal")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            AppError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "too_many_requests"),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::Internal(e) => {
                tracing::error!("internal: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        let mut body = json!({ "error": code });

        match &self {
            AppError::Validation(msg) => body["detail"] = json!(msg),
            AppError::Conflict(msg) => body["detail"] = json!(msg),
            AppError::RateLimited { retry_after } => body["retry_after"] = json!(retry_after),
            _ => {}
        }

        (status, Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
