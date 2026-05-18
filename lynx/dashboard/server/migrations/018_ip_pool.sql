-- WireGuard management plane IP pool (10.100.0.0/16).
-- ip = NULL agent_id → free; filled → assigned to that agent.
CREATE TABLE ip_pool (
    ip         INET         PRIMARY KEY,
    agent_id   UUID         REFERENCES agents(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Pre-populate the usable range 10.100.0.2 – 10.100.0.254 (first /24).
-- The dashboard is always 10.100.0.1; extend with additional INSERTs as needed.
INSERT INTO ip_pool (ip)
SELECT ('10.100.0.' || g)::INET
FROM generate_series(2, 254) AS g;
