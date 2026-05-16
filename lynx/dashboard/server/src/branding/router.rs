use super::handlers;
use crate::state::AppState;
use axum::{routing::get, Router};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(handlers::get_branding).put(handlers::update_branding))
}
