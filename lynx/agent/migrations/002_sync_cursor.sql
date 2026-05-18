-- Tracks which audit_log entries have been synced to the dashboard.
-- Simpler than adding a column to audit_log: a single-row cursor table.

CREATE TABLE sync_state (
    id              INTEGER     PRIMARY KEY DEFAULT 1 CHECK (id = 1),  -- singleton
    last_synced_at  TIMESTAMPTZ NOT NULL DEFAULT 'epoch'
);

INSERT INTO sync_state (last_synced_at) VALUES ('epoch');
