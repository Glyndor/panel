CREATE TABLE sessions (
    id                  UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ip                  TEXT        NOT NULL,
    user_agent          TEXT,
    refresh_token_hash  TEXT        NOT NULL UNIQUE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at          TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_sessions_user_id            ON sessions(user_id);
CREATE INDEX idx_sessions_refresh_token_hash ON sessions(refresh_token_hash);
CREATE INDEX idx_sessions_expires_at         ON sessions(expires_at);
