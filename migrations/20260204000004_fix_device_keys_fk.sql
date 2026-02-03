-- Fix foreign key constraint in device_keys table
-- Date: 2026-02-04

DO $$
BEGIN
    -- 1. Drop the incorrect foreign key constraint if it exists
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'device_keys_user_id_device_id_fkey') THEN
        ALTER TABLE device_keys DROP CONSTRAINT device_keys_user_id_device_id_fkey;
    END IF;

    -- 2. Add the corrected foreign key constraint
    -- Note: devices table PK is (device_id, user_id)
    ALTER TABLE device_keys 
    ADD CONSTRAINT device_keys_device_user_fkey 
    FOREIGN KEY (device_id, user_id) 
    REFERENCES devices(device_id, user_id) 
    ON DELETE CASCADE;

END $$;
