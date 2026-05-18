use super::{BrandingRow, UpdateBrandingRequest};
use crate::{error::AppError, state::AppState};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};

// --------------------------------------------------------------------------
// GET /branding — public, no auth required
// --------------------------------------------------------------------------

pub async fn get_branding(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as!(
        BrandingRow,
        "SELECT company_name, logo_url, primary_color, secondary_color, accent_color, updated_at FROM white_label WHERE id = 1"
    )
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_else(|| BrandingRow {
        company_name: "Lynx".into(),
        logo_url: None,
        primary_color: "#0f172a".into(),
        secondary_color: "#38bdf8".into(),
        accent_color: "#6366f1".into(),
        updated_at: chrono::Utc::now(),
    });

    Ok(Json(row))
}

// --------------------------------------------------------------------------
// PUT /branding — requires auth (admin only in practice, enforced by route_layer)
// --------------------------------------------------------------------------

pub async fn update_branding(
    State(state): State<AppState>,
    Json(req): Json<UpdateBrandingRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate company name length.
    if let Some(ref name) = req.company_name {
        if name.len() > 255 {
            return Err(AppError::Validation(
                "company_name must be ≤ 255 characters".into(),
            ));
        }
    }

    // Validate logo_url: must be https:// and not excessively long.
    if let Some(ref url) = req.logo_url {
        if !url.starts_with("https://") {
            return Err(AppError::Validation(
                "logo_url must start with https://".into(),
            ));
        }
        if url.len() > 2048 {
            return Err(AppError::Validation(
                "logo_url must be ≤ 2048 characters".into(),
            ));
        }
    }

    // Validate hex colors if provided
    for (field, val) in [
        ("primary_color", &req.primary_color),
        ("secondary_color", &req.secondary_color),
        ("accent_color", &req.accent_color),
    ] {
        if let Some(v) = val {
            if !v.starts_with('#') || v.len() != 7 {
                return Err(AppError::Validation(format!(
                    "{field}: must be a 7-char hex color like #0f172a"
                )));
            }
        }
    }

    sqlx::query!(
        r#"
        INSERT INTO white_label (id, company_name, logo_url, primary_color, secondary_color, accent_color, updated_at)
        VALUES (1,
            COALESCE($1, 'Lynx'),
            $2,
            COALESCE($3, '#0f172a'),
            COALESCE($4, '#38bdf8'),
            COALESCE($5, '#6366f1'),
            NOW()
        )
        ON CONFLICT (id) DO UPDATE SET
            company_name    = COALESCE($1, white_label.company_name),
            logo_url        = COALESCE($2, white_label.logo_url),
            primary_color   = COALESCE($3, white_label.primary_color),
            secondary_color = COALESCE($4, white_label.secondary_color),
            accent_color    = COALESCE($5, white_label.accent_color),
            updated_at      = NOW()
        "#,
        req.company_name,
        req.logo_url,
        req.primary_color,
        req.secondary_color,
        req.accent_color,
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
