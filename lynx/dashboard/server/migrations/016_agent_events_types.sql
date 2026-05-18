-- Extend agent_events with event types discovered during implementation.
-- The original CHECK constraint was too narrow; ALTER TABLE ... ADD CONSTRAINT is the
-- cleanest way to add values to an existing CHECK without data migration.

ALTER TABLE agent_events DROP CONSTRAINT IF EXISTS agent_events_event_check;

ALTER TABLE agent_events ADD CONSTRAINT agent_events_event_check CHECK (event IN (
    'connected',
    'disconnected',
    'lockdown',
    'heartbeat_lost',
    'rebooting',
    'update_applied',
    'nftables_divergence',
    'bootstrap_completed',
    'conflicting_software_detected',
    'nginx_unexpected_stop',
    'nginx_config_tampered',
    'mtls_cert_expired',
    'audit_integrity_failure',
    'wg_offline'
));
