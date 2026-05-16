-- Organizations and projects managed by this Lynx instance.

CREATE TABLE organizations (
    id           UUID        PRIMARY KEY,
    name         TEXT        NOT NULL,
    slug         TEXT        NOT NULL UNIQUE,
    owner_id     UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_organizations_owner ON organizations(owner_id);

-- Organization members (users can belong to multiple orgs)
CREATE TABLE organization_members (
    organization_id  UUID        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id          UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role             TEXT        NOT NULL DEFAULT 'member'
                                 CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    joined_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, user_id)
);

-- Projects belong to an organization and target an agent
CREATE TABLE projects (
    id               UUID        PRIMARY KEY,
    organization_id  UUID        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    agent_id         UUID        NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    name             TEXT        NOT NULL,
    slug             TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (organization_id, slug)
);

CREATE INDEX idx_projects_organization ON projects(organization_id);
CREATE INDEX idx_projects_agent        ON projects(agent_id);
