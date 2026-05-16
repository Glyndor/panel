-- Append-only. No FK to sessions — logs outlive sessions.
CREATE TABLE session_logs (
    id          UUID        PRIMARY KEY,
    session_id  UUID        NOT NULL,
    reason      TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_session_logs_session_id ON session_logs(session_id);
