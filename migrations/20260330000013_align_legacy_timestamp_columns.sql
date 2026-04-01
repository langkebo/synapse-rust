DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE device_trust_status RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE device_trust_status RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE cross_signing_trust RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE cross_signing_trust RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE verification_requests RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'verification_requests' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE verification_requests RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE device_verification_request RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE e2ee_security_events RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE secure_key_backups RENAME COLUMN created_at TO created_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE secure_key_backups RENAME COLUMN updated_at TO updated_ts;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys' AND column_name = 'created_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE secure_backup_session_keys RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE device_trust_status
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_trust_status'
          AND column_name = 'updated_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE device_trust_status
        ALTER COLUMN updated_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM updated_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE cross_signing_trust
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'cross_signing_trust'
          AND column_name = 'updated_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE cross_signing_trust
        ALTER COLUMN updated_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM updated_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'device_verification_request'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE device_verification_request
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'e2ee_security_events'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE e2ee_security_events
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE secure_key_backups
        ALTER COLUMN created_ts DROP DEFAULT;
        ALTER TABLE secure_key_backups
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_key_backups'
          AND column_name = 'updated_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE secure_key_backups
        ALTER COLUMN updated_ts DROP DEFAULT;
        ALTER TABLE secure_key_backups
        ALTER COLUMN updated_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM updated_ts) * 1000)::BIGINT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'secure_backup_session_keys'
          AND column_name = 'created_ts' AND data_type <> 'bigint'
    ) THEN
        ALTER TABLE secure_backup_session_keys
        ALTER COLUMN created_ts DROP DEFAULT;
        ALTER TABLE secure_backup_session_keys
        ALTER COLUMN created_ts TYPE BIGINT
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;
END $$;

ALTER TABLE IF EXISTS device_trust_status ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS device_trust_status ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS cross_signing_trust ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS cross_signing_trust ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS verification_requests ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS verification_requests ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS device_verification_request ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS e2ee_security_events ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN updated_ts DROP NOT NULL;
ALTER TABLE IF EXISTS secure_key_backups ALTER COLUMN updated_ts SET DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT;
ALTER TABLE IF EXISTS secure_backup_session_keys ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE IF EXISTS secure_backup_session_keys ALTER COLUMN created_ts SET DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT;

DROP INDEX IF EXISTS idx_verification_requests_to_user_state;
CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state
ON verification_requests(to_user, state, updated_ts DESC);

DROP INDEX IF EXISTS idx_e2ee_security_events_user_created;
CREATE INDEX IF NOT EXISTS idx_e2ee_security_events_user_created
ON e2ee_security_events(user_id, created_ts DESC);

DROP INDEX IF EXISTS idx_secure_key_backups_user;
CREATE INDEX IF NOT EXISTS idx_secure_key_backups_user
ON secure_key_backups(user_id, created_ts DESC);
