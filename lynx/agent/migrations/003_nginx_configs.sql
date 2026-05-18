CREATE TABLE nginx_configs (
    id             UUID        PRIMARY KEY DEFAULT uuidv7(),
    config_content TEXT        NOT NULL,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
