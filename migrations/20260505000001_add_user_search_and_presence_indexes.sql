-- Case-insensitive directory search on username.
CREATE INDEX IF NOT EXISTS idx_users_lower_username ON users (LOWER(username));

-- Search code uses LOWER(COALESCE(displayname, '')), so index the same expression.
CREATE INDEX IF NOT EXISTS idx_users_lower_displayname
    ON users (LOWER(COALESCE(displayname, '')));

-- Support exact/prefix email lookup in directory search.
CREATE INDEX IF NOT EXISTS idx_users_lower_email
    ON users (LOWER(COALESCE(email, '')));

-- Friend list and search fall back to created_ts ordering.
CREATE INDEX IF NOT EXISTS idx_users_created_ts ON users (created_ts DESC);

-- Presence joins for friend list projection.
CREATE INDEX IF NOT EXISTS idx_presence_user_id ON presence (user_id);
