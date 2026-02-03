-- Fix device_keys table constraints to match code expectations
-- Date: 2026-02-02

DO $$
BEGIN
    -- 1. Ensure columns exist (repair if necessary)
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'id') THEN
        ALTER TABLE device_keys ADD COLUMN id UUID DEFAULT gen_random_uuid();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'display_name') THEN
        ALTER TABLE device_keys ADD COLUMN display_name TEXT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'algorithm') THEN
        ALTER TABLE device_keys ADD COLUMN algorithm TEXT NOT NULL DEFAULT 'm.olm.v1.curve25519-aes-sha2';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'key_id') THEN
        ALTER TABLE device_keys ADD COLUMN key_id TEXT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'public_key') THEN
        ALTER TABLE device_keys ADD COLUMN public_key TEXT;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'signatures') THEN
        ALTER TABLE device_keys ADD COLUMN signatures JSONB NOT NULL DEFAULT '{}';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'created_at') THEN
        ALTER TABLE device_keys ADD COLUMN created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'updated_at') THEN
        ALTER TABLE device_keys ADD COLUMN updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW();
    END IF;

    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'device_keys' AND column_name = 'ts_updated_ms') THEN
        ALTER TABLE device_keys ADD COLUMN ts_updated_ms BIGINT DEFAULT (extract(epoch from now()) * 1000);
    END IF;

    -- 2. Ensure the UNIQUE constraint exists for ON CONFLICT
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'device_keys_user_id_device_id_key_id_key') THEN
        ALTER TABLE device_keys ADD CONSTRAINT device_keys_user_id_device_id_key_id_key UNIQUE(user_id, device_id, key_id);
    END IF;

END $$;
