DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_members'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'joined_member_count'
    ) THEN
        ALTER TABLE room_summaries RENAME COLUMN joined_members TO joined_member_count;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_members'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_summaries' AND column_name = 'invited_member_count'
    ) THEN
        ALTER TABLE room_summaries RENAME COLUMN invited_members TO invited_member_count;
    END IF;
END $$;

ALTER TABLE room_summaries
    ADD COLUMN IF NOT EXISTS id BIGSERIAL,
    ADD COLUMN IF NOT EXISTS room_type TEXT,
    ADD COLUMN IF NOT EXISTS avatar_url TEXT,
    ADD COLUMN IF NOT EXISTS join_rules TEXT NOT NULL DEFAULT 'invite',
    ADD COLUMN IF NOT EXISTS history_visibility TEXT NOT NULL DEFAULT 'shared',
    ADD COLUMN IF NOT EXISTS guest_access TEXT NOT NULL DEFAULT 'forbidden',
    ADD COLUMN IF NOT EXISTS is_direct BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_space BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS joined_member_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS invited_member_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_event_id TEXT,
    ADD COLUMN IF NOT EXISTS last_event_ts BIGINT,
    ADD COLUMN IF NOT EXISTS last_message_ts BIGINT,
    ADD COLUMN IF NOT EXISTS unread_notifications BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS unread_highlight BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS created_ts BIGINT NOT NULL DEFAULT 0;

UPDATE room_summaries
SET hero_users = '[]'::jsonb
WHERE hero_users IS NULL;

UPDATE room_summaries
SET updated_ts = 0
WHERE updated_ts IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_room_summaries_id_unique
ON room_summaries(id);

CREATE INDEX IF NOT EXISTS idx_room_summaries_last_event_ts
ON room_summaries(last_event_ts DESC);

CREATE INDEX IF NOT EXISTS idx_room_summaries_space
ON room_summaries(is_space)
WHERE is_space = TRUE;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_room_summaries_room'
    ) THEN
        ALTER TABLE room_summaries
        ADD CONSTRAINT fk_room_summaries_room
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    membership TEXT NOT NULL,
    is_hero BOOLEAN NOT NULL DEFAULT FALSE,
    last_active_ts BIGINT,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_summary_members_room_user UNIQUE (room_id, user_id),
    CONSTRAINT fk_room_summary_members_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_summary_members_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_user_membership_room
ON room_summary_members(user_id, membership, room_id);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_membership_hero_active
ON room_summary_members(room_id, membership, is_hero DESC, last_active_ts DESC);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_hero_user
ON room_summary_members(room_id, is_hero DESC, user_id);

ALTER TABLE server_retention_policy
    ADD COLUMN IF NOT EXISTS max_lifetime BIGINT,
    ADD COLUMN IF NOT EXISTS min_lifetime BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'server_retention_policy'
          AND column_name = 'max_lifetime_days'
    ) AND EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'server_retention_policy'
          AND column_name = 'min_lifetime_days'
    ) THEN
        EXECUTE $stmt$
            UPDATE server_retention_policy
            SET
                max_lifetime = COALESCE(max_lifetime, max_lifetime_days::BIGINT * 86400000),
                min_lifetime = COALESCE(min_lifetime, min_lifetime_days::BIGINT * 86400000),
                updated_ts = COALESCE(updated_ts, created_ts, 0)
            WHERE
                max_lifetime IS NULL
                OR min_lifetime = 0
                OR updated_ts IS NULL
        $stmt$;
    ELSE
        UPDATE server_retention_policy
        SET updated_ts = COALESCE(updated_ts, created_ts, 0)
        WHERE updated_ts IS NULL;
    END IF;
END
$$;

INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
VALUES (1, NULL, 0, FALSE, 0, 0)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    max_lifetime BIGINT,
    min_lifetime BIGINT NOT NULL DEFAULT 0,
    expire_on_clients BOOLEAN NOT NULL DEFAULT FALSE,
    is_server_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_room_retention_policies_room UNIQUE (room_id),
    CONSTRAINT fk_room_retention_policies_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_retention_policies_server_default
ON room_retention_policies(is_server_default)
WHERE is_server_default = TRUE;
