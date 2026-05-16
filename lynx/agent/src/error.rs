use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(&'static str),
    #[error("bad request: {0}")]
    BadRequest(&'static str),
    #[error("lockdown active")]
    Lockdown,
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AgentError>;

impl IntoResponse for AgentError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AgentError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AgentError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            AgentError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AgentError::Lockdown => (StatusCode::SERVICE_UNAVAILABLE, "lockdown"),
            AgentError::Internal(e) => {
                error!("internal: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        (status, Json(json!({ "error": code }))).into_response()
    }
}
