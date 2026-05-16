use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::list_agents).post(handlers::register_agent))
        .route("/{id}", get(handlers::get_agent).delete(handlers::remove_agent))
        .route("/{id}/heartbeat", post(handlers::relay_heartbeat))
        .route("/{id}/cmd", post(handlers::send_command))
}
