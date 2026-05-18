-- Permissions: fixed set, seeded here, never editable by users
CREATE TABLE permissions (
    id         UUID        PRIMARY KEY,
    key        TEXT        NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Roles: created by admins (except the bootstrap admin role)
CREATE TABLE roles (
    id         UUID        PRIMARY KEY,
    name       TEXT        NOT NULL UNIQUE,
    created_by UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Many-to-many: which permissions belong to a role
CREATE TABLE role_permissions (
    id            UUID        PRIMARY KEY,
    role_id       UUID        NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID        NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_by    UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(role_id, permission_id)
);

-- Many-to-many: which roles a user has
CREATE TABLE user_roles (
    id         UUID        PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id    UUID        NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_by UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, role_id)
);

CREATE INDEX idx_role_permissions_role_id ON role_permissions(role_id);
CREATE INDEX idx_user_roles_user_id       ON user_roles(user_id);
CREATE INDEX idx_user_roles_role_id       ON user_roles(role_id);

-- Seed all permissions (fixed — never change these keys)
INSERT INTO permissions (id, key) VALUES
    (uuidv7(), 'vps:read'),
    (uuidv7(), 'vps:create'),
    (uuidv7(), 'vps:edit'),
    (uuidv7(), 'vps:delete'),
    (uuidv7(), 'vps:*'),
    (uuidv7(), 'org:read'),
    (uuidv7(), 'org:create'),
    (uuidv7(), 'org:edit'),
    (uuidv7(), 'org:delete'),
    (uuidv7(), 'org:*'),
    (uuidv7(), 'project:read'),
    (uuidv7(), 'project:create'),
    (uuidv7(), 'project:edit'),
    (uuidv7(), 'project:delete'),
    (uuidv7(), 'project:*'),
    (uuidv7(), '*:*');
