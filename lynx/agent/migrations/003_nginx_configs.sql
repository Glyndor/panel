CREATE TABLE nginx_configs (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    config_content TEXT        NOT NULL,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
