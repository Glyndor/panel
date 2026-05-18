use super::{build_jwt_keys, extract_ip, extract_ua};
use crate::{
    auth::{models::RefreshRequest, rate_limit, session},
    crypto::{hash, jwt},
    error::{AppError, Result},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use base64ct::{Base64UrlUnpadded, Encoding};
use uuid::Uuid;

use crate::auth::models::TokenResponse;

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let keys = build_jwt_keys(&state);
    let claims = jwt::verify_access_token(&keys, token).map_err(|_| AppError::Unauthorized)?;

    let mut redis = state.redis.clone();

    if !session::check_jti_valid(&mut redis, claims.jti)
        .await
        .map_err(anyhow::Error::from)?
    {
        return Err(AppError::Unauthorized);
    }

    session::revoke_access_jti(&mut redis, claims.jti)
        .await
        .map_err(anyhow::Error::from)?;

    session::delete_by_session_id(&state.db, claims.session_id)
        .await
        .map_err(anyhow::Error::from)?;

    session::log_event(&state.db, claims.session_id, "user_logout")
        .await
        .map_err(anyhow::Error::from)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>> {
    let ip = extract_ip(&headers);
    let ua = extract_ua(&headers);
    let mut redis = state.redis.clone();
    rate_limit::check_refresh(&mut redis, &ip).await?;

    let token_bytes =
        Base64UrlUnpadded::decode_vec(&body.refresh_token).map_err(|_| AppError::Unauthorized)?;
    let token_hash = hash::token_hash(&token_bytes, &state.config.pepper);

    let record = session::find_by_refresh_hash(&state.db, &token_hash)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or(AppError::Unauthorized)?;

    let new_refresh_raw = session::gen_refresh_token();
    let new_refresh_hash = hash::token_hash(&new_refresh_raw, &state.config.pepper);

    let jti = Uuid::now_v7();

    let rotated =
        session::rotate_refresh(&state.db, record.id, &token_hash, &new_refresh_hash, jti)
            .await
            .map_err(anyhow::Error::from)?;

    if !rotated {
        return Err(AppError::Unauthorized);
    }

    let keys = build_jwt_keys(&state);
    let access_token = jwt::issue_access_token(
        &keys,
        record.user_id,
        jti,
        record.id,
        &hash::ip_hash(&ip),
        &hash::ua_hash(&ua),
    )
    .map_err(anyhow::Error::from)?;

    session::store_access_jti(&mut redis, jti, record.id)
        .await
        .map_err(anyhow::Error::from)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token: Base64UrlUnpadded::encode_string(&new_refresh_raw),
        expires_in: 900,
        force_password_change: false,
    }))
}
