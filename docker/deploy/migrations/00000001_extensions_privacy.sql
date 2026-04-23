-- =============================================================================
-- Extension: Privacy Settings (feature: privacy-ext)
-- Tables: user_privacy_settings
-- =============================================================================

CREATE TABLE IF NOT EXISTS user_privacy_settings (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    profile_visibility TEXT NOT NULL DEFAULT 'public',
    avatar_visibility TEXT NOT NULL DEFAULT 'public',
    displayname_visibility TEXT NOT NULL DEFAULT 'public',
    presence_visibility TEXT NOT NULL DEFAULT 'contacts',
    room_membership_visibility TEXT NOT NULL DEFAULT 'contacts',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT user_privacy_settings_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_privacy_settings_user ON user_privacy_settings(user_id);
