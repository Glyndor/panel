-- Data-plane WireGuard tunnels between agents for cross-agent horizontal scaling.
-- Each row represents a tunnel between two agents for a specific project.
-- Distinct from the management plane (dashboard <-> agent).

CREATE TABLE data_plane_tunnels (
    id              UUID        PRIMARY KEY,
    project_id      UUID        NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_a_id      UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    agent_b_id      UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    -- WireGuard keypairs (public keys only stored; private keys live on agents)
    agent_a_pubkey  TEXT        NOT NULL,
    agent_b_pubkey  TEXT        NOT NULL,
    -- WireGuard IPs for the data-plane tunnel (distinct address space)
    agent_a_wg_ip   TEXT        NOT NULL,
    agent_b_wg_ip   TEXT        NOT NULL,
    wg_port         INTEGER     NOT NULL DEFAULT 51821,
    -- Replica count on agent_b for this project
    replica_count   INTEGER     NOT NULL DEFAULT 1,
    status          TEXT        NOT NULL DEFAULT 'pending'
                                CHECK (status IN ('pending', 'active', 'error', 'torn_down')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, agent_b_id)
);

CREATE INDEX idx_data_plane_project ON data_plane_tunnels(project_id);
CREATE INDEX idx_data_plane_agents ON data_plane_tunnels(agent_a_id, agent_b_id);
