use crate::config::Config;
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Set to true when heartbeat is lost — agent enters lockdown
    pub lockdown: Arc<AtomicBool>,
    /// Last known-good nftables checksum after apply(). None = no ruleset applied yet.
    pub nft_checksum: Arc<Mutex<Option<String>>>,
    /// Rendered nft ruleset from last successful apply() — used for restore.
    pub nft_last_ruleset: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn is_locked_down(&self) -> bool {
        self.lockdown.load(Ordering::SeqCst)
    }

    pub fn set_nft_checksum(&self, checksum: String) {
        *self.nft_checksum.lock().unwrap() = Some(checksum);
    }

    pub fn expected_nft_checksum(&self) -> Option<String> {
        self.nft_checksum.lock().unwrap().clone()
    }

    pub fn set_nft_last_ruleset(&self, ruleset: String) {
        *self.nft_last_ruleset.lock().unwrap() = Some(ruleset);
    }

    pub fn nft_last_ruleset(&self) -> Option<String> {
        self.nft_last_ruleset.lock().unwrap().clone()
    }
}
