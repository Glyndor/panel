-- Add output chain state entries so agent can persist and restore output rules across reboots.

-- Extend the check constraint to allow the two output-chain variants.
ALTER TABLE nftables_state DROP CONSTRAINT nftables_state_chain_check;
ALTER TABLE nftables_state ADD CONSTRAINT nftables_state_chain_check
    CHECK (chain IN ('lynx-global', 'lynx-local', 'lynx-global-output', 'lynx-local-output'));

INSERT INTO nftables_state (chain, body, wg_port) VALUES
    ('lynx-global-output', '', 51820),
    ('lynx-local-output',  '', 51820)
ON CONFLICT DO NOTHING;
