-- Key-value store for system-level metadata (e.g. setup_token_issued_at).
CREATE TABLE system_config (
    key        TEXT        PRIMARY KEY,
    value      TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
