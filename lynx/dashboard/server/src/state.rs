use crate::config::Config;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub config: Arc<Config>,
    /// Latest agent version known from GitHub, refreshed hourly.
    pub latest_agent_version: Arc<RwLock<Option<String>>>,
}
