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

CREATE TABLE IF NOT EXISTS space_summaries (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL UNIQUE,
    summary JSONB DEFAULT '{}',
    children_count BIGINT DEFAULT 0,
    member_count BIGINT DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT fk_space_summary_space FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE
);

