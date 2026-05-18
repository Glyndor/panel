use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::get_domain).post(handlers::set_domain))
        .route("/verify", post(handlers::verify_domain))
        .route("/hsts", post(handlers::set_hsts))
        .route("/close-port", post(handlers::close_port))
        .route("/cert/upload", post(handlers::upload_cert))
}
