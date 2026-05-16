use super::handlers;
use crate::state::AppState;
use axum::{routing::post, Router};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
        .route("/logout", post(handlers::logout))
        .route("/refresh", post(handlers::refresh))
}
