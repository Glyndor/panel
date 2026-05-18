-- Persists the last-applied nftables chain bodies so the agent can
-- re-apply rules after reboot without waiting for a dashboard push.
CREATE TABLE nftables_state (
    chain       TEXT        PRIMARY KEY CHECK (chain IN ('lynx-global', 'lynx-local')),
    body        TEXT        NOT NULL DEFAULT '',
    wg_port     INTEGER     NOT NULL DEFAULT 51820,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed default empty bodies so the rows always exist.
INSERT INTO nftables_state (chain, body, wg_port) VALUES
    ('lynx-global', '', 51820),
    ('lynx-local',  '', 51820)
ON CONFLICT DO NOTHING;
