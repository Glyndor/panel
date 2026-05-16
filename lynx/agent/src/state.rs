use crate::config::Config;
use sqlx::PgPool;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Set to true when heartbeat is lost — agent enters lockdown
    pub lockdown: Arc<AtomicBool>,
}

impl AppState {
    pub fn is_locked_down(&self) -> bool {
        self.lockdown.load(Ordering::SeqCst)
    }
}
