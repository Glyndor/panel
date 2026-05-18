-- Records all key rotation events (manual + automatic).
-- Per CLAUDE.md: triggered_by null = automatic (update/scheduled)

CREATE TABLE rotation_log (
    id           UUID        PRIMARY KEY,
    triggered_by UUID        REFERENCES users(id) ON DELETE SET NULL,
    reason       TEXT        NOT NULL CHECK (reason IN ('update', 'manual', 'scheduled', 'emergency')),
    scope        TEXT        NOT NULL CHECK (scope IN ('jwt_keys', 'wireguard_psks', 'all', 'certificates')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rotation_log_created_at ON rotation_log(created_at DESC);
