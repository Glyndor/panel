-- Agents registered with this dashboard instance.
-- One row per agent VPS.

CREATE TABLE agents (
    id               UUID        PRIMARY KEY,               -- UUID v7, set by agent
    name             TEXT        NOT NULL,
    wg_pubkey        TEXT        NOT NULL UNIQUE,
    wg_ip            TEXT        NOT NULL UNIQUE,             -- e.g. "10.100.0.2"
    wg_endpoint      TEXT,                                   -- agent VPS public IP:port (optional)
    api_port         INTEGER     NOT NULL DEFAULT 9090,
    status           TEXT        NOT NULL DEFAULT 'offline'
                                 CHECK (status IN ('online', 'lockdown', 'offline')),
    version          TEXT,
    last_heartbeat   TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agents_status ON agents(status);

-- Per CLAUDE.md: agent connection/state events (dashboard-side)
CREATE TABLE agent_events (
    id         UUID        PRIMARY KEY,
    agent_id   UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    event      TEXT        NOT NULL
                           CHECK (event IN (
                               'connected', 'disconnected', 'lockdown',
                               'heartbeat_lost', 'update_applied',
                               'nftables_divergence', 'bootstrap_completed'
                           )),
    detail     TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agent_events_agent_id ON agent_events(agent_id);
CREATE INDEX idx_agent_events_created_at ON agent_events(created_at);
