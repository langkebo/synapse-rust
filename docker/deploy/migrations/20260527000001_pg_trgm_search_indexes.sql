-- Create pg_trgm extension for trigram-based text search optimization.
-- This enables GIN indexes that support ILIKE and % (similarity) operators,
-- significantly improving performance for user and room search queries.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Trigram indexes for users table (used by user search queries)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'users'
          AND column_name = 'name'
    ) THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_users_name_trgm ON users USING gin (name gin_trgm_ops)';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'users'
          AND column_name = 'user_id'
    ) THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_users_user_id_trgm ON users USING gin (user_id gin_trgm_ops)';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'users'
          AND column_name = 'username'
    ) THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_users_username_trgm ON users USING gin (username gin_trgm_ops)';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'users'
          AND column_name = 'displayname'
    ) THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_users_displayname_trgm ON users USING gin (displayname gin_trgm_ops)';
    END IF;
END $$;

-- Trigram indexes for rooms table (used by room search queries)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'rooms'
          AND column_name = 'name'
    ) THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_rooms_name_trgm ON rooms USING gin (name gin_trgm_ops)';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = 'rooms'
          AND column_name = 'canonical_alias'
    ) THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_rooms_canonical_alias_trgm ON rooms USING gin (canonical_alias gin_trgm_ops)';
    END IF;
END $$;
