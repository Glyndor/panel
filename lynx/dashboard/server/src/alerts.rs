use sqlx::PgPool;
use uuid::Uuid;

pub async fn fire(db: &PgPool, kind: &str, detail: impl Into<Option<String>>, agent_id: impl Into<Option<Uuid>>) {
    let id = Uuid::now_v7();
    let detail = detail.into();
    let agent_id = agent_id.into();

    let _ = sqlx::query!(
        "INSERT INTO security_alerts (id, kind, detail, agent_id) VALUES ($1, $2, $3, $4)",
        id,
        kind,
        detail,
        agent_id,
    )
    .execute(db)
    .await;

    tracing::warn!(kind, ?detail, ?agent_id, "security alert fired");
}
