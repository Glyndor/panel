-- Tracks all auto-update and manual update events across agents.

CREATE TABLE update_log (
    id           UUID        PRIMARY KEY,
    triggered_by UUID        REFERENCES users(id) ON DELETE SET NULL,
    version      TEXT        NOT NULL,
    channel      TEXT        NOT NULL DEFAULT 'stable' CHECK (channel IN ('stable', 'edge')),
    scope        TEXT        NOT NULL CHECK (scope IN ('dashboard', 'agent', 'all')),
    agent_id     UUID        REFERENCES agents(id) ON DELETE SET NULL,
    status       TEXT        NOT NULL CHECK (status IN ('pending', 'success', 'failed')),
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_update_log_created_at ON update_log(created_at DESC);
CREATE INDEX idx_update_log_agent_id   ON update_log(agent_id);
