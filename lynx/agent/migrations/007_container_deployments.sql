-- Track compose deployments so the agent can restart containers on reboot.
-- Each row is one project deployment with its desired state.

CREATE TABLE container_deployments (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    TEXT        NOT NULL,
    project_id   TEXT        NOT NULL,
    compose_path TEXT        NOT NULL,
    desired      TEXT        NOT NULL CHECK (desired IN ('running', 'stopped')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, project_id)
);

CREATE INDEX idx_container_deployments_desired ON container_deployments(desired);
