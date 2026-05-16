-- Singleton row for dashboard domain configuration.
-- id is forced to 1 via CHECK to ensure exactly one row.

CREATE TABLE domain_config (
    id              INTEGER     PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    domain          TEXT,
    cert_type       TEXT        NOT NULL DEFAULT 'self_signed'
                                CHECK (cert_type IN ('self_signed', 'lets_encrypt')),
    cert_expires_at TIMESTAMPTZ,
    hsts_enabled    BOOLEAN     NOT NULL DEFAULT false,
    port_19443_open BOOLEAN     NOT NULL DEFAULT true,
    status          TEXT        NOT NULL DEFAULT 'unconfigured'
                                CHECK (status IN ('unconfigured', 'pending', 'active', 'error')),
    error_message   TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO domain_config DEFAULT VALUES;
