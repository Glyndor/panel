use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::list_orgs).post(handlers::create_org))
        .route("/{id}", get(handlers::get_org).delete(handlers::delete_org))
        .route("/{id}/members", get(handlers::list_members).post(handlers::invite_member))
        .route("/{id}/members/{user_id}", delete(handlers::remove_member))
        .route("/{id}/projects", get(handlers::list_projects))
        .route("/{id}/projects/{proj_id}", get(handlers::get_project))
        .route("/{id}/projects/{proj_id}/resources", put(handlers::update_container_resources))
}
