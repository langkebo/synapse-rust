DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_state'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_state
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_users'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_users
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_events
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_transactions'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_transactions
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_user_namespaces'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_user_namespaces
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_room_namespaces'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_room_namespaces
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_room_alias_namespaces'
          AND column_name = 'as_id'
          AND udt_name = 'text'
    ) THEN
        ALTER TABLE application_service_room_alias_namespaces
        ALTER COLUMN as_id TYPE VARCHAR(255);
    END IF;
END $$;

UPDATE application_service_state
SET appservice_id = as_id
WHERE (appservice_id IS NULL OR appservice_id = '')
  AND as_id IS NOT NULL
  AND as_id <> '';

UPDATE application_service_users
SET appservice_id = as_id
WHERE (appservice_id IS NULL OR appservice_id = '')
  AND as_id IS NOT NULL
  AND as_id <> '';

UPDATE application_service_events
SET appservice_id = as_id
WHERE (appservice_id IS NULL OR appservice_id = '')
  AND as_id IS NOT NULL
  AND as_id <> '';

UPDATE application_service_user_namespaces
SET as_id = appservice_id
WHERE (as_id IS NULL OR as_id = '')
  AND appservice_id IS NOT NULL
  AND appservice_id <> '';

ALTER TABLE application_service_state
ALTER COLUMN appservice_id SET NOT NULL;

ALTER TABLE application_service_users
ALTER COLUMN appservice_id SET NOT NULL;

ALTER TABLE application_service_events
ALTER COLUMN appservice_id SET NOT NULL;

ALTER TABLE application_service_events
ALTER COLUMN processed SET DEFAULT FALSE;

UPDATE application_service_events
SET processed = FALSE
WHERE processed IS NULL;

ALTER TABLE application_service_events
ALTER COLUMN processed SET NOT NULL;

ALTER TABLE application_service_events
ALTER COLUMN as_id SET NOT NULL;

ALTER TABLE application_service_user_namespaces
ALTER COLUMN as_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'chk_appservice_state_id_consistency'
    ) THEN
        ALTER TABLE application_service_state
        ADD CONSTRAINT chk_appservice_state_id_consistency
        CHECK (appservice_id = as_id);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'chk_appservice_users_id_consistency'
    ) THEN
        ALTER TABLE application_service_users
        ADD CONSTRAINT chk_appservice_users_id_consistency
        CHECK (appservice_id = as_id);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'chk_appservice_events_id_consistency'
    ) THEN
        ALTER TABLE application_service_events
        ADD CONSTRAINT chk_appservice_events_id_consistency
        CHECK (appservice_id = as_id);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_state_as_id'
    ) THEN
        ALTER TABLE application_service_state
        ADD CONSTRAINT fk_application_service_state_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_state
        VALIDATE CONSTRAINT fk_application_service_state_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_users_as_id'
    ) THEN
        ALTER TABLE application_service_users
        ADD CONSTRAINT fk_application_service_users_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_users
        VALIDATE CONSTRAINT fk_application_service_users_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_events_as_id'
    ) THEN
        ALTER TABLE application_service_events
        ADD CONSTRAINT fk_application_service_events_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_events
        VALIDATE CONSTRAINT fk_application_service_events_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_statistics_as_id'
    ) THEN
        ALTER TABLE application_service_statistics
        ADD CONSTRAINT fk_application_service_statistics_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_statistics
        VALIDATE CONSTRAINT fk_application_service_statistics_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_transactions_as_id'
    ) THEN
        ALTER TABLE application_service_transactions
        ADD CONSTRAINT fk_application_service_transactions_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_transactions
        VALIDATE CONSTRAINT fk_application_service_transactions_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_user_namespaces_as_id'
    ) THEN
        ALTER TABLE application_service_user_namespaces
        ADD CONSTRAINT fk_application_service_user_namespaces_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_user_namespaces
        VALIDATE CONSTRAINT fk_application_service_user_namespaces_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_room_namespaces_as_id'
    ) THEN
        ALTER TABLE application_service_room_namespaces
        ADD CONSTRAINT fk_application_service_room_namespaces_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_room_namespaces
        VALIDATE CONSTRAINT fk_application_service_room_namespaces_as_id;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_application_service_room_alias_namespaces_as_id'
    ) THEN
        ALTER TABLE application_service_room_alias_namespaces
        ADD CONSTRAINT fk_application_service_room_alias_namespaces_as_id
        FOREIGN KEY (as_id) REFERENCES application_services(as_id)
        ON DELETE CASCADE NOT VALID;
        ALTER TABLE application_service_room_alias_namespaces
        VALIDATE CONSTRAINT fk_application_service_room_alias_namespaces_as_id;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_application_service_state_as_id ON application_service_state(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_users_as_id ON application_service_users(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_as_id ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as_id ON application_service_transactions(as_id);
