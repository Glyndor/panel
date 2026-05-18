-- nftables rule management
-- scope: 'global' (all agents) or 'local' (specific agent)
-- kind: type of rule

CREATE TABLE nftables_rules (
    id           UUID PRIMARY KEY,
    scope        TEXT        NOT NULL CHECK (scope IN ('global', 'local')),
    agent_id     UUID        REFERENCES agents(id) ON DELETE CASCADE,
    kind         TEXT        NOT NULL CHECK (kind IN ('allow_port', 'block_port', 'allow_ip', 'block_ip', 'rate_limit')),
    port         INTEGER,
    protocol     TEXT        CHECK (protocol IN ('tcp', 'udp', 'both')),
    ip_list      TEXT[]      NOT NULL DEFAULT '{}',
    ip_version   TEXT        NOT NULL DEFAULT 'both' CHECK (ip_version IN ('ipv4', 'ipv6', 'both')),
    rate_per_min INTEGER,
    description  TEXT,
    priority     INTEGER     NOT NULL DEFAULT 0,
    enabled      BOOLEAN     NOT NULL DEFAULT true,
    created_by   UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT local_needs_agent  CHECK (scope = 'global' OR agent_id IS NOT NULL),
    CONSTRAINT port_rule_has_port CHECK (
        kind NOT IN ('allow_port', 'block_port', 'rate_limit') OR port IS NOT NULL
    ),
    CONSTRAINT rate_limit_has_rate CHECK (
        kind != 'rate_limit' OR rate_per_min IS NOT NULL
    )
);

CREATE INDEX idx_nftables_rules_scope    ON nftables_rules(scope);
CREATE INDEX idx_nftables_rules_agent_id ON nftables_rules(agent_id);
CREATE INDEX idx_nftables_rules_enabled  ON nftables_rules(enabled);

-- Track sync status of global rules per agent
CREATE TABLE global_rule_sync (
    rule_id    UUID        NOT NULL REFERENCES nftables_rules(id) ON DELETE CASCADE,
    agent_id   UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    synced_at  TIMESTAMPTZ,
    PRIMARY KEY (rule_id, agent_id)
);
