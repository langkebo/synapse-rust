DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE device_trust_status RENAME COLUMN created_ts TO created_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'updated_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'updated_at'
    ) THEN
        ALTER TABLE device_trust_status RENAME COLUMN updated_ts TO updated_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE cross_signing_trust RENAME COLUMN created_ts TO created_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'updated_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'updated_at'
    ) THEN
        ALTER TABLE cross_signing_trust RENAME COLUMN updated_ts TO updated_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE verification_requests RENAME COLUMN created_ts TO created_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'updated_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'updated_at'
    ) THEN
        ALTER TABLE verification_requests RENAME COLUMN updated_ts TO updated_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE device_verification_request RENAME COLUMN created_ts TO created_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE e2ee_security_events RENAME COLUMN created_ts TO created_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE secure_key_backups RENAME COLUMN created_ts TO created_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'updated_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'updated_at'
    ) THEN
        ALTER TABLE secure_key_backups RENAME COLUMN updated_ts TO updated_at;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE secure_backup_session_keys RENAME COLUMN created_ts TO created_at;
    END IF;
END $$;
