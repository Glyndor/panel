use crate::{
    auth::middleware::AuthUser,
    error::{AppError, Result},
    state::AppState,
};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

pub async fn get_preferences(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse> {
    let prefs = sqlx::query!(
        "SELECT theme, locale FROM user_preferences WHERE user_id = $1",
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?;

    let (theme, locale) = match prefs {
        Some(p) => (p.theme, p.locale),
        None => ("system".to_string(), "en".to_string()),
    };

    Ok(Json(
        serde_json::json!({ "theme": theme, "locale": locale }),
    ))
}

#[derive(Deserialize)]
pub struct UpdatePreferencesRequest {
    pub theme: Option<String>,
    pub locale: Option<String>,
}

#[derive(Deserialize)]
pub struct SingleSessionRequest {
    pub enabled: bool,
}

pub async fn update_single_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<SingleSessionRequest>,
) -> Result<StatusCode> {
    sqlx::query!(
        "UPDATE users SET single_session = $1 WHERE id = $2",
        body.enabled,
        user.user_id
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_preferences(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<UpdatePreferencesRequest>,
) -> Result<impl IntoResponse> {
    let valid_themes = ["light", "dark", "system"];
    let valid_locales = ["en", "es"];

    if let Some(ref t) = body.theme {
        if !valid_themes.contains(&t.as_str()) {
            return Err(AppError::Validation(
                "theme must be light, dark, or system".into(),
            ));
        }
    }
    if let Some(ref l) = body.locale {
        if !valid_locales.contains(&l.as_str()) {
            return Err(AppError::Validation("unsupported locale".into()));
        }
    }

    sqlx::query!(
        r#"INSERT INTO user_preferences (user_id, theme, locale)
           VALUES ($1, COALESCE($2, 'system'), COALESCE($3, 'en'))
           ON CONFLICT (user_id) DO UPDATE SET
               theme = COALESCE($2, user_preferences.theme),
               locale = COALESCE($3, user_preferences.locale),
               updated_at = NOW()"#,
        user.user_id,
        body.theme,
        body.locale,
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
