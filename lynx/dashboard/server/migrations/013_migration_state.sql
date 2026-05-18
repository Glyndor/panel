-- Tracks dashboard-to-dashboard migration state.
-- Only one migration can be active at a time (singleton row).

CREATE TABLE migration_state (
    id                  INTEGER     PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    -- 'idle' | 'preparing' | 'transferring' | 'notifying_agents' | 'waiting_agents'
    -- | 'completed' | 'aborted' | 'error'
    status              TEXT        NOT NULL DEFAULT 'idle',
    role                TEXT        NOT NULL DEFAULT 'none'
                                    CHECK (role IN ('none', 'source', 'target')),
    -- VPS-A (source): URL of VPS-B
    target_url          TEXT,
    -- VPS-B (target): one-time token shown to admin; stored as hash
    migration_token_hash TEXT,
    agents_total        INTEGER     NOT NULL DEFAULT 0,
    agents_confirmed    INTEGER     NOT NULL DEFAULT 0,
    error_message       TEXT,
    started_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO migration_state DEFAULT VALUES;
