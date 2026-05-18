-- Security alerts — written on detection, read by frontend via WS or polling.
-- All alerts are append-only; acknowledged_at marks admin has seen it.
CREATE TABLE security_alerts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    kind            TEXT        NOT NULL,   -- rate_limit_hit, intercepted, nftables_divergence, etc.
    detail          TEXT,
    agent_id        UUID        REFERENCES agents(id) ON DELETE SET NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged_at TIMESTAMPTZ
);

CREATE INDEX security_alerts_unacked ON security_alerts (created_at)
    WHERE acknowledged_at IS NULL;
