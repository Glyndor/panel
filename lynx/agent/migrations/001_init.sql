-- Agent-local PostgreSQL schema

CREATE TABLE audit_log (
    id               UUID        PRIMARY KEY,
    agent_id         UUID        NOT NULL,
    organization_id  UUID,
    user_id          UUID,
    command_type     TEXT        NOT NULL,
    result           TEXT        NOT NULL CHECK (result IN ('success', 'rejected', 'failed')),
    error            TEXT,
    previous_hash    TEXT        NOT NULL,
    entry_hash       TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_agent_id    ON audit_log(agent_id);
CREATE INDEX idx_audit_log_created_at  ON audit_log(created_at);

-- Replay protection: Ed25519 command nonces seen in last 60s
CREATE TABLE used_nonces (
    nonce      TEXT        PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Auto-expire nonces older than 60 seconds (cleaned by agent on startup + periodic)
CREATE INDEX idx_used_nonces_created_at ON used_nonces(created_at);
