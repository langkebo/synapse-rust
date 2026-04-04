DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'room_id'
    ) THEN
        ALTER TABLE spaces ADD COLUMN room_id TEXT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'join_rule'
    ) THEN
        ALTER TABLE spaces ADD COLUMN join_rule TEXT DEFAULT 'invite';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'visibility'
    ) THEN
        ALTER TABLE spaces ADD COLUMN visibility TEXT DEFAULT 'private';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'parent_space_id'
    ) THEN
        ALTER TABLE spaces ADD COLUMN parent_space_id TEXT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'spaces' AND column_name = 'join_rules'
    ) THEN
        EXECUTE $sql$
            UPDATE spaces
            SET join_rule = COALESCE(join_rule, join_rules, 'invite')
            WHERE join_rule IS NULL
        $sql$;
    ELSE
        UPDATE spaces
        SET join_rule = COALESCE(join_rule, 'invite')
        WHERE join_rule IS NULL;
    END IF;
END $$;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_spaces_parent ON spaces(parent_space_id) WHERE parent_space_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS space_members (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL DEFAULT 'join',
    joined_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    left_ts BIGINT,
    inviter TEXT,
    CONSTRAINT uq_space_members_space_user UNIQUE (space_id, user_id)
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_members_space ON space_members(space_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_members_user ON space_members(user_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_members_membership ON space_members(membership);

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_summary_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_summary_space ON space_summaries(space_id);

CREATE TABLE IF NOT EXISTS space_statistics (
    space_id TEXT PRIMARY KEY,
    name TEXT,
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    child_room_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_statistics_member_count ON space_statistics(member_count DESC);

CREATE TABLE IF NOT EXISTS space_events (
    event_id TEXT NOT NULL PRIMARY KEY,
    space_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    CONSTRAINT fk_space_events_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_events_space ON space_events(space_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_events_space_type_ts
ON space_events(space_id, event_type, origin_server_ts DESC);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_space_events_space_ts
ON space_events(space_id, origin_server_ts DESC);
