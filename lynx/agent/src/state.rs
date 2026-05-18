use crate::config::Config;
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
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
    /// Body of the lynx-global chain (input, managed by dashboard global rules).
    pub nft_global_body: Arc<Mutex<String>>,
    /// Body of the lynx-local chain (input, managed by dashboard local rules for this agent).
    pub nft_local_body: Arc<Mutex<String>>,
    /// Body of the lynx-global-output chain (output, managed by dashboard global rules).
    pub nft_global_output_body: Arc<Mutex<String>>,
    /// Body of the lynx-local-output chain (output, managed by dashboard local rules for this agent).
    pub nft_local_output_body: Arc<Mutex<String>>,
    /// WireGuard port used in the last full nftables apply (stored for chain-only updates).
    pub nft_wg_port: Arc<std::sync::atomic::AtomicU32>,
    /// In-memory command rate limiter: (window_start_secs, count_in_window)
    pub cmd_rate: Arc<Mutex<(u64, u64)>>,
    /// Count of `rejected_rate_limit` events in the current minute — alert threshold.
    pub cmd_rejected_count: Arc<AtomicU64>,
    /// Epoch-second when the current rejection-count minute window started.
    pub cmd_rejected_window: Arc<AtomicU64>,
}

impl AppState {
    pub fn is_locked_down(&self) -> bool {
        self.lockdown.load(Ordering::SeqCst)
    }

    /// Returns true if the command is within the 100/min limit, false if it should be rejected.
    pub fn check_cmd_rate(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut guard = self.cmd_rate.lock().unwrap();
        let (window_start, count) = *guard;
        if now >= window_start + 60 {
            *guard = (now, 1);
            true
        } else if count < 100 {
            guard.1 += 1;
            true
        } else {
            false
        }
    }

    /// Record a rejected-rate-limit event. Returns count in current minute.
    pub fn record_rate_rejection(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let window = self.cmd_rejected_window.load(Ordering::SeqCst);
        if now >= window + 60 {
            self.cmd_rejected_window.store(now, Ordering::SeqCst);
            self.cmd_rejected_count.store(1, Ordering::SeqCst);
            1
        } else {
            self.cmd_rejected_count.fetch_add(1, Ordering::SeqCst) + 1
        }
    }

    pub fn nft_wg_port(&self) -> u16 {
        self.nft_wg_port.load(Ordering::SeqCst) as u16
    }

    pub fn set_nft_wg_port(&self, port: u16) {
        self.nft_wg_port.store(port as u32, Ordering::SeqCst);
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

    pub fn set_nft_global_body(&self, body: String) {
        *self.nft_global_body.lock().unwrap() = body;
    }

    pub fn nft_global_body(&self) -> String {
        self.nft_global_body.lock().unwrap().clone()
    }

    pub fn set_nft_local_body(&self, body: String) {
        *self.nft_local_body.lock().unwrap() = body;
    }

    pub fn nft_local_body(&self) -> String {
        self.nft_local_body.lock().unwrap().clone()
    }

    pub fn set_nft_global_output_body(&self, body: String) {
        *self.nft_global_output_body.lock().unwrap() = body;
    }

    pub fn nft_global_output_body(&self) -> String {
        self.nft_global_output_body.lock().unwrap().clone()
    }

    pub fn set_nft_local_output_body(&self, body: String) {
        *self.nft_local_output_body.lock().unwrap() = body;
    }

    pub fn nft_local_output_body(&self) -> String {
        self.nft_local_output_body.lock().unwrap().clone()
    }
}
