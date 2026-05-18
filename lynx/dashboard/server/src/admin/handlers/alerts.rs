use crate::{error::AppError, state::AppState};
use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct AlertRow {
    pub id: Uuid,
    pub kind: String,
    pub detail: Option<String>,
    pub agent_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_alerts(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query_as!(
        AlertRow,
        "SELECT id, kind, detail, agent_id, created_at
         FROM security_alerts
         WHERE acknowledged_at IS NULL
         ORDER BY created_at DESC
         LIMIT 100"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

pub async fn acknowledge_alert(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query!(
        "UPDATE security_alerts SET acknowledged_at = NOW() WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
