-- Central audit log — aggregated from all agents.
-- Hash chain integrity is verified on the dashboard side during sync.

CREATE TABLE audit_log (
    id               UUID        PRIMARY KEY,
    agent_id         UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    organization_id  UUID,
    user_id          UUID,
    command_type     TEXT        NOT NULL,
    result           TEXT        NOT NULL CHECK (result IN ('success', 'rejected', 'failed')),
    error            TEXT,
    previous_hash    TEXT        NOT NULL,
    entry_hash       TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL,
    synced_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_agent_id    ON audit_log(agent_id);
CREATE INDEX idx_audit_log_created_at  ON audit_log(created_at);
CREATE INDEX idx_audit_log_user_id     ON audit_log(user_id) WHERE user_id IS NOT NULL;

-- Per-agent sync token (SHA-256 hash, for agent→dashboard audit sync auth)
ALTER TABLE agents
    ADD COLUMN sync_token_hash TEXT;
