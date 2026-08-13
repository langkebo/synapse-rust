-- Align retention policy boolean columns with v10 is_ prefix convention.
-- v7: expire_on_clients BOOLEAN  →  v10 / Rust: is_expire_on_clients BOOLEAN
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_retention_policies'
          AND column_name = 'expire_on_clients'
    ) THEN
        ALTER TABLE room_retention_policies RENAME COLUMN expire_on_clients TO is_expire_on_clients;
    END IF;
END $$;

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'server_retention_policy'
          AND column_name = 'expire_on_clients'
    ) THEN
        ALTER TABLE server_retention_policy RENAME COLUMN expire_on_clients TO is_expire_on_clients;
    END IF;
END $$;
