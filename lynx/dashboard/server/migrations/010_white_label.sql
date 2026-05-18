-- White-label branding stored in PostgreSQL.
-- Single row (id=1 constraint). Updated via dashboard admin UI.

CREATE TABLE white_label (
    id              INTEGER     PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    company_name    TEXT        NOT NULL DEFAULT 'Lynx',
    logo_url        TEXT,
    primary_color   TEXT        NOT NULL DEFAULT '#0f172a',
    secondary_color TEXT        NOT NULL DEFAULT '#38bdf8',
    accent_color    TEXT        NOT NULL DEFAULT '#6366f1',
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO white_label (id) VALUES (1);
