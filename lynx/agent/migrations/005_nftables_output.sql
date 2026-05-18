-- Add output chain state entries so agent can persist and restore output rules across reboots.
INSERT INTO nftables_state (chain, body, wg_port) VALUES
    ('lynx-global-output', '', 51820),
    ('lynx-local-output',  '', 51820)
ON CONFLICT DO NOTHING;
