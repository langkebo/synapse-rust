-- Rename cas_services boolean columns for v10 is_ prefix alignment
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cas_services'
          AND column_name = 'require_secure'
    ) THEN
        ALTER TABLE cas_services RENAME COLUMN require_secure TO is_require_secure;
    END IF;
END $$;

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cas_services'
          AND column_name = 'single_logout'
    ) THEN
        ALTER TABLE cas_services RENAME COLUMN single_logout TO is_single_logout;
    END IF;
END $$;
