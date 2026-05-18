use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Public auth routes — no require_auth middleware.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
        .route("/logout", post(handlers::logout))
        .route("/refresh", post(handlers::refresh))
        .route("/me", get(handlers::me))
        .route("/change-password", post(handlers::change_password))
}

/// Protected auth routes — require_auth applied by main.rs.
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/me/preferences", get(handlers::get_preferences))
        .route("/me/preferences", post(handlers::update_preferences))
}
