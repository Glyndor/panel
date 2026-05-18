-- Per-user UI preferences: theme and locale
CREATE TABLE user_preferences (
    user_id  UUID        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    theme    TEXT        NOT NULL DEFAULT 'system', -- 'light' | 'dark' | 'system'
    locale   TEXT        NOT NULL DEFAULT 'en',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
