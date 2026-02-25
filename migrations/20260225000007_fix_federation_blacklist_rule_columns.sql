-- Fix federation_blacklist_rule table column names
-- created_at -> created_ts, updated_at -> updated_ts

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'created_ts') THEN
            ALTER TABLE federation_blacklist_rule RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed federation_blacklist_rule.created_at to created_ts';
        END IF;
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'updated_ts') THEN
            ALTER TABLE federation_blacklist_rule RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed federation_blacklist_rule.updated_at to updated_ts';
        END IF;
    END IF;

    -- Add missing columns if not exist
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_blacklist_rule' AND column_name = 'created_by') THEN
        ALTER TABLE federation_blacklist_rule ADD COLUMN created_by VARCHAR(255) DEFAULT 'system';
        RAISE NOTICE 'Added created_by column to federation_blacklist_rule';
    END IF;

    RAISE NOTICE 'federation_blacklist_rule table schema updated';
END $$;
