use crate::config::Config;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub config: Arc<Config>,
    /// Latest agent version known from GitHub, refreshed hourly.
    pub latest_agent_version: Arc<RwLock<Option<String>>>,
    /// Per-agent WireGuard PSKs in memory (agent_id → base64 PSK).
    /// Loaded at startup from Podman secret files; updated when agents register/rotate.
    pub wg_psks: Arc<RwLock<HashMap<Uuid, Zeroizing<String>>>>,
}
