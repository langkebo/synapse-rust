-- Migration: Fix application-service schema compatibility and Matrix-aligned data types
-- Version: 20260305000004
-- Date: 2026-03-05

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services'
          AND column_name = 'protocols'
          AND data_type = 'ARRAY'
    ) THEN
        ALTER TABLE application_services
        ALTER COLUMN protocols TYPE JSONB USING to_jsonb(protocols);
    END IF;
END $$;

ALTER TABLE application_services
ALTER COLUMN protocols SET DEFAULT '[]'::jsonb;

ALTER TABLE application_services
ALTER COLUMN namespaces SET DEFAULT '{}'::jsonb;

ALTER TABLE application_service_state
ADD COLUMN IF NOT EXISTS state_key TEXT;

ALTER TABLE application_service_state
ADD COLUMN IF NOT EXISTS state_value TEXT;

ALTER TABLE application_service_state
ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_state' AND column_name = 'key'
    ) THEN
        EXECUTE 'UPDATE application_service_state SET state_key = COALESCE(state_key, key) WHERE state_key IS NULL';
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_state' AND column_name = 'value'
    ) THEN
        EXECUTE 'UPDATE application_service_state SET state_value = COALESCE(state_value, value) WHERE state_value IS NULL';
    END IF;
END $$;

UPDATE application_service_state
SET state_key = COALESCE(state_key, '__default__'),
    state_value = COALESCE(state_value, ''),
    updated_ts = COALESCE(updated_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
WHERE state_key IS NULL OR state_value IS NULL OR updated_ts IS NULL;

ALTER TABLE application_service_state
ALTER COLUMN state_key SET NOT NULL;

ALTER TABLE application_service_state
ALTER COLUMN state_value SET NOT NULL;

ALTER TABLE application_service_state
ALTER COLUMN updated_ts SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_application_service_state_as_id_state_key
ON application_service_state(as_id, state_key);

ALTER TABLE application_service_users
ADD COLUMN IF NOT EXISTS displayname TEXT;

ALTER TABLE application_service_users
ADD COLUMN IF NOT EXISTS avatar_url TEXT;

ALTER TABLE application_service_users
ADD COLUMN IF NOT EXISTS created_ts BIGINT;

UPDATE application_service_users
SET created_ts = COALESCE(created_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
WHERE created_ts IS NULL;

ALTER TABLE application_service_users
ALTER COLUMN created_ts SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_application_service_users_as_id_user_id
ON application_service_users(as_id, user_id);
