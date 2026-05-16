-- Agent certificates issued by dashboard CA.
-- Stored as serialized SignedCert JSON so agents can verify on first connection.

ALTER TABLE agents
    ADD COLUMN cert_payload   TEXT,
    ADD COLUMN cert_signature TEXT,
    ADD COLUMN cert_expires_at TIMESTAMPTZ;
