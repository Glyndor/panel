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
    headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_default()
}

pub(super) fn extract_ua(headers: &HeaderMap) -> String {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}
