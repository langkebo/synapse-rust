-- ============================================================================
-- Forward Script: 20260515000006_consolidated_constraint_governance_v7.sql
-- Description: Adds missing composite primary keys and core foreign keys for
--              presence and auth/reporting tables.
-- Renamed from: 20260520000011_add_constraint_governance.sql
-- Created: 2026-05-20
-- Updated: 2026-05-09 (renamed for consistency)
-- ============================================================================

SET TIME ZONE 'UTC';

-- Missing PK: typing
ALTER TABLE IF EXISTS typing
    DROP CONSTRAINT IF EXISTS typing_user_id_room_id_key;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'typing'
          AND constraint_name = 'pk_typing'
    ) THEN
        ALTER TABLE typing
            ADD CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id);
    END IF;
END $$;

-- Missing PK: presence_subscriptions
ALTER TABLE IF EXISTS presence_subscriptions
    DROP CONSTRAINT IF EXISTS uq_presence_subscriptions;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'presence_subscriptions'
          AND constraint_name = 'pk_presence_subscriptions'
    ) THEN
        ALTER TABLE presence_subscriptions
            ADD CONSTRAINT pk_presence_subscriptions PRIMARY KEY (subscriber_id, target_id);
    END IF;
END $$;

-- Support FK enforcement paths that currently lack dedicated indexes.
CREATE INDEX IF NOT EXISTS idx_access_tokens_device_id
ON access_tokens(device_id)
WHERE device_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_device_id
ON refresh_tokens(device_id)
WHERE device_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_refresh_token_families_device
ON refresh_token_families(device_id)
WHERE device_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_token_blacklist_user_id
ON token_blacklist(user_id)
WHERE user_id IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_access_tokens_device'
    ) THEN
        ALTER TABLE access_tokens
            ADD CONSTRAINT fk_access_tokens_device
            FOREIGN KEY (device_id) REFERENCES devices(device_id)
            ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_tokens_device'
    ) THEN
        ALTER TABLE refresh_tokens
            ADD CONSTRAINT fk_refresh_tokens_device
            FOREIGN KEY (device_id) REFERENCES devices(device_id)
            ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_token_blacklist_user'
    ) THEN
        ALTER TABLE token_blacklist
            ADD CONSTRAINT fk_token_blacklist_user
            FOREIGN KEY (user_id) REFERENCES users(user_id)
            ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_registration_token_usage_user'
    ) THEN
        ALTER TABLE registration_token_usage
            ADD CONSTRAINT fk_registration_token_usage_user
            FOREIGN KEY (user_id) REFERENCES users(user_id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_report_rate_limits_user'
    ) THEN
        ALTER TABLE report_rate_limits
            ADD CONSTRAINT fk_report_rate_limits_user
            FOREIGN KEY (user_id) REFERENCES users(user_id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_usage_token'
    ) THEN
        ALTER TABLE refresh_token_usage
            ADD CONSTRAINT fk_refresh_token_usage_token
            FOREIGN KEY (refresh_token_id) REFERENCES refresh_tokens(id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_usage_user'
    ) THEN
        ALTER TABLE refresh_token_usage
            ADD CONSTRAINT fk_refresh_token_usage_user
            FOREIGN KEY (user_id) REFERENCES users(user_id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_families_user'
    ) THEN
        ALTER TABLE refresh_token_families
            ADD CONSTRAINT fk_refresh_token_families_user
            FOREIGN KEY (user_id) REFERENCES users(user_id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_families_device'
    ) THEN
        ALTER TABLE refresh_token_families
            ADD CONSTRAINT fk_refresh_token_families_device
            FOREIGN KEY (device_id) REFERENCES devices(device_id)
            ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_refresh_token_rotations_family'
    ) THEN
        ALTER TABLE refresh_token_rotations
            ADD CONSTRAINT fk_refresh_token_rotations_family
            FOREIGN KEY (family_id) REFERENCES refresh_token_families(family_id)
            ON DELETE CASCADE NOT VALID;
    END IF;
END $$;

INSERT INTO schema_migrations (version, name, is_success, description, applied_ts)
VALUES (
    '20260515000006',
    'consolidated_constraint_governance_v7',
    TRUE,
    'Add missing composite primary keys and core foreign keys for presence/auth tables',
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
)
ON CONFLICT (version) DO NOTHING;
