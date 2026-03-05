DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_state'
          AND column_name = 'appservice_id'
          AND is_nullable = 'NO'
    ) THEN
        ALTER TABLE application_service_state
        ALTER COLUMN appservice_id DROP NOT NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_users'
          AND column_name = 'appservice_id'
          AND is_nullable = 'NO'
    ) THEN
        ALTER TABLE application_service_users
        ALTER COLUMN appservice_id DROP NOT NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events'
          AND column_name = 'appservice_id'
          AND is_nullable = 'NO'
    ) THEN
        ALTER TABLE application_service_events
        ALTER COLUMN appservice_id DROP NOT NULL;
    END IF;
END $$;

ALTER TABLE application_service_state
DROP CONSTRAINT IF EXISTS chk_appservice_state_id_consistency;

ALTER TABLE application_service_users
DROP CONSTRAINT IF EXISTS chk_appservice_users_id_consistency;

ALTER TABLE application_service_events
DROP CONSTRAINT IF EXISTS chk_appservice_events_id_consistency;
