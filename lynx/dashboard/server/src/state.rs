use crate::config::Config;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, oneshot, RwLock};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Live WebSocket connection from a connected agent.
pub struct AgentWsConn {
    /// Send outbound messages (text frames) to the agent.
    pub sender: tokio::sync::mpsc::UnboundedSender<axum::extract::ws::Message>,
    /// Pending command responses keyed by request UUID.
    pub pending: Arc<tokio::sync::Mutex<HashMap<Uuid, oneshot::Sender<serde_json::Value>>>>,
}

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
    /// Active WebSocket connections from agents (agent_id → connection).
    pub agent_ws_conns: Arc<RwLock<HashMap<Uuid, Arc<AgentWsConn>>>>,
    /// Per-agent broadcast channels for real-time metric fan-out to frontend WS clients.
    /// Keyed by agent_id. Channels are created on agent connect, dropped on disconnect.
    /// Wrapped in Arc so cleanup can use ptr_eq to avoid removing a newer session's entry.
    pub agent_metric_tx: Arc<RwLock<HashMap<Uuid, Arc<broadcast::Sender<Arc<String>>>>>>,
    /// Global broadcast channel for agent events pushed to all subscribed admin browser sessions.
    pub events_tx: Arc<broadcast::Sender<Arc<String>>>,
}
