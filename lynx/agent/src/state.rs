use crate::config::Config;
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

/// Tracks why the agent entered lockdown.
/// Only `Heartbeat` (and `None`) can be cleared by a `heartbeat_ack`.
/// All other reasons require a manual service restart to clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockdownReason {
    Heartbeat,
    PgUnreachable,
    IncompatibleSoftware,
    NftablesFailure,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Set to true when the agent enters lockdown.
    pub lockdown: Arc<AtomicBool>,
    /// The reason the agent entered lockdown, if any.
    pub lockdown_reason: Arc<Mutex<Option<LockdownReason>>>,
    /// Last known-good nftables checksum after apply(). None = no ruleset applied yet.
    pub nft_checksum: Arc<Mutex<Option<String>>>,
    /// Per-chain checksums captured after each successful apply() — used for divergence attribution.
    pub nft_chain_checksums: Arc<Mutex<[Option<String>; 3]>>,
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
    /// Epoch-second of last successful dashboard contact (WS connect or message received).
    /// 0 = never connected. Used by the fallback updater to detect dashboard absence.
    pub last_dashboard_contact: Arc<AtomicU64>,
    /// Instant of last received heartbeat ACK from dashboard.
    /// Reset by both the HTTP /heartbeat handler and the WS heartbeat_ack path.
    /// The lockdown watchdog fires when this exceeds HEARTBEAT_TIMEOUT_SECS.
    pub last_heartbeat: Arc<Mutex<Instant>>,
}

impl AppState {
    pub fn is_locked_down(&self) -> bool {
        self.lockdown.load(Ordering::SeqCst)
    }

    /// Enter lockdown with an explicit reason.
    pub fn set_lockdown(&self, reason: LockdownReason) {
        self.lockdown.store(true, Ordering::SeqCst);
        *self.lockdown_reason.lock().unwrap() = Some(reason);
    }

    /// Clear lockdown only when the reason is `Heartbeat` or `None`.
    /// Reasons such as `PgUnreachable`, `IncompatibleSoftware`, and
    /// `NftablesFailure` require a manual service restart to clear.
    pub fn clear_lockdown_if_heartbeat(&self) {
        let mut guard = self.lockdown_reason.lock().unwrap();
        match *guard {
            None | Some(LockdownReason::Heartbeat) => {
                self.lockdown.store(false, Ordering::SeqCst);
                *guard = None;
            }
            _ => {}
        }
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

    /// Store per-chain checksums: (base, global, local).
    pub fn set_nft_chain_checksums(
        &self,
        base: Option<String>,
        global: Option<String>,
        local: Option<String>,
    ) {
        let mut g = self.nft_chain_checksums.lock().unwrap();
        g[0] = base;
        g[1] = global;
        g[2] = local;
    }

    /// Expected chain checksum by index: 0=base, 1=global, 2=local.
    pub fn expected_chain_checksum(&self, idx: usize) -> Option<String> {
        self.nft_chain_checksums.lock().unwrap()[idx].clone()
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
