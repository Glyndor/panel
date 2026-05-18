-- Extend cert_type CHECK constraint to include 'cloudflare' and 'custom' types
-- that the backend already accepts but were missing from the original schema.

ALTER TABLE domain_config
    DROP CONSTRAINT IF EXISTS domain_config_cert_type_check;

ALTER TABLE domain_config
    ADD CONSTRAINT domain_config_cert_type_check
        CHECK (cert_type IN ('self_signed', 'lets_encrypt', 'cloudflare', 'custom'));
