mod login;
mod me;
mod password;
mod preferences;
mod register;
mod session;

pub use login::login;
pub use me::me;
pub use password::change_password;
pub use preferences::{get_preferences, update_preferences, update_single_session};
pub use register::register;
pub use session::{logout, refresh};

use crate::{crypto::jwt, state::AppState};
use axum::http::HeaderMap;

pub(super) fn build_jwt_keys(state: &AppState) -> jwt::JwtKeys {
    jwt::JwtKeys {
        sign_private_seed: *state.config.jwt_sign_private_seed,
        sign_public_bytes: state.config.jwt_sign_public_bytes,
        enc_private_bytes: *state.config.jwt_enc_private_bytes,
        enc_public_bytes: state.config.jwt_enc_public_bytes,
    }
}

pub(super) fn extract_ip(headers: &HeaderMap) -> String {
    let raw = headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .or_else(|| headers.get("x-peer-addr"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    // x-forwarded-for may contain a comma-separated list; prefer IPv4 over IPv6.
    let mut ipv4: Option<&str> = None;
    let mut first: Option<&str> = None;
    for candidate in raw.split(',') {
        let s = candidate.trim();
        if first.is_none() {
            first = Some(s);
        }
        if s.contains('.') && !s.contains(':') && ipv4.is_none() {
            ipv4 = Some(s);
        }
    }
    ipv4.or(first).unwrap_or_default().to_string()
}

pub(super) fn extract_ua(headers: &HeaderMap) -> String {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}
