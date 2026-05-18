CREATE TABLE users (
    id              UUID        PRIMARY KEY,
    username        TEXT        NOT NULL UNIQUE,
    email_hash      TEXT        NOT NULL UNIQUE,
    email_encrypted BYTEA       NOT NULL,
    password_hash   TEXT        NOT NULL,
    dek_encrypted   BYTEA       NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_username   ON users(username);
CREATE INDEX idx_users_email_hash ON users(email_hash);
