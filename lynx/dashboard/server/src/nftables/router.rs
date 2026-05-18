use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Global rules
        .route(
            "/global",
            get(handlers::list_global_rules).post(handlers::create_global_rule),
        )
        .route("/global/push", post(handlers::push_global_rules))
        .route("/global/{id}", delete(handlers::delete_global_rule))
        // Local rules (per agent)
        .route(
            "/agents/{agent_id}/local",
            get(handlers::list_local_rules).post(handlers::create_local_rule),
        )
        .route(
            "/agents/{agent_id}/local/push",
            post(handlers::push_local_rules),
        )
        .route(
            "/agents/{agent_id}/local/{rule_id}",
            delete(handlers::delete_local_rule),
        )
}
