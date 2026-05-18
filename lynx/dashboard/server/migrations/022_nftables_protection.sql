-- Add port range support (for X11 6000-6063, BitTorrent 6881-6889, etc.)
ALTER TABLE nftables_rules ADD COLUMN port_end INTEGER;

-- Add direction to distinguish input vs output chain rules
ALTER TABLE nftables_rules
    ADD COLUMN direction TEXT NOT NULL DEFAULT 'input'
    CHECK (direction IN ('input', 'output'));

-- Extend kind constraint
ALTER TABLE nftables_rules DROP CONSTRAINT nftables_rules_kind_check;
ALTER TABLE nftables_rules ADD CONSTRAINT nftables_rules_kind_check CHECK (
    kind IN (
        'allow_port', 'block_port', 'allow_ip', 'block_ip', 'rate_limit',
        'drop_invalid_state', 'tcp_flag_null', 'tcp_flag_xmas', 'tcp_flag_ack_new',
        'icmp_ping_limit', 'allow_icmp_errors', 'allow_ndp',
        'block_output_port'
    )
);

-- Default global protection rules — input chain
INSERT INTO nftables_rules
    (id, scope, kind, direction, ip_version, rate_per_min, description, priority, enabled)
VALUES
    (gen_random_uuid(), 'global', 'drop_invalid_state', 'input', 'both', NULL, 'Drop invalid connection states',                 10, true),
    (gen_random_uuid(), 'global', 'tcp_flag_null',       'input', 'both', NULL, 'Drop NULL scan (no TCP flags set)',             20, true),
    (gen_random_uuid(), 'global', 'tcp_flag_xmas',       'input', 'both', NULL, 'Drop XMAS scan (FIN+PSH+URG)',                 30, true),
    (gen_random_uuid(), 'global', 'tcp_flag_ack_new',    'input', 'both', NULL, 'Drop ACK on new connection',                   40, true),
    (gen_random_uuid(), 'global', 'icmp_ping_limit',     'input', 'both', 3,    'Rate limit ICMP/ICMPv6 echo (3/s, burst 10)', 50, true),
    (gen_random_uuid(), 'global', 'allow_icmp_errors',   'input', 'both', NULL, 'Accept ICMP error types (PMTUD, path MTU)',   60, true),
    (gen_random_uuid(), 'global', 'allow_ndp',           'input', 'ipv6', NULL, 'Accept IPv6 Neighbor Discovery Protocol',      70, true);

-- Default global protection rules — output chain (prevent spam relay / abuse)
INSERT INTO nftables_rules
    (id, scope, kind, direction, port, port_end, protocol, ip_version, description, priority, enabled)
VALUES
    (gen_random_uuid(), 'global', 'block_output_port', 'output',  25,   NULL, 'tcp',  'both', 'Block outbound SMTP (25/tcp)',              100, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 465,   NULL, 'tcp',  'both', 'Block outbound SMTPS (465/tcp)',            101, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 587,   NULL, 'tcp',  'both', 'Block outbound submission (587/tcp)',       102, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output',  23,   NULL, 'tcp',  'both', 'Block outbound Telnet (23/tcp)',            110, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output',  20,   NULL, 'tcp',  'both', 'Block outbound FTP data (20/tcp)',          120, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output',  21,   NULL, 'tcp',  'both', 'Block outbound FTP control (21/tcp)',       121, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 137,   NULL, 'udp',  'both', 'Block outbound NetBIOS NS (137/udp)',       130, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 138,   NULL, 'udp',  'both', 'Block outbound NetBIOS DG (138/udp)',       131, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 139,   NULL, 'tcp',  'both', 'Block outbound NetBIOS SS (139/tcp)',       132, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 445,   NULL, 'tcp',  'both', 'Block outbound SMB (445/tcp)',              133, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 6667,  NULL, 'tcp',  'both', 'Block outbound IRC (6667/tcp)',             140, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 111,   NULL, 'tcp',  'both', 'Block outbound RPC (111/tcp)',              150, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 111,   NULL, 'udp',  'both', 'Block outbound RPC (111/udp)',              151, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output',  69,   NULL, 'udp',  'both', 'Block outbound TFTP (69/udp)',              160, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 6000, 6063, 'tcp',  'both', 'Block outbound X11 (6000-6063/tcp)',        170, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 1080,  NULL, 'tcp',  'both', 'Block outbound SOCKS (1080/tcp)',           180, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 389,   NULL, 'tcp',  'both', 'Block outbound LDAP (389/tcp)',             190, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 636,   NULL, 'tcp',  'both', 'Block outbound LDAPS (636/tcp)',            191, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 5353,  NULL, 'udp',  'both', 'Block outbound mDNS (5353/udp)',            200, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 6881, 6889, 'tcp',  'both', 'Block outbound BitTorrent TCP (6881-6889)', 210, true),
    (gen_random_uuid(), 'global', 'block_output_port', 'output', 6881, 6889, 'udp',  'both', 'Block outbound BitTorrent UDP (6881-6889)', 211, true);
