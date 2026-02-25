-- 统一时间戳列命名规范迁移脚本（第三阶段）
-- 处理剩余的表

DO $$
BEGIN
    -- 1. retention_policies 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'retention_policies' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'retention_policies' AND column_name = 'updated_ts') THEN
            ALTER TABLE retention_policies RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed retention_policies.updated_at to updated_ts';
        END IF;
    END IF;

    -- 2. saml_identity_providers 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'created_ts') THEN
            ALTER TABLE saml_identity_providers RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed saml_identity_providers.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_identity_providers' AND column_name = 'updated_ts') THEN
            ALTER TABLE saml_identity_providers RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed saml_identity_providers.updated_at to updated_ts';
        END IF;
    END IF;

    -- 3. saml_sessions 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_sessions' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'saml_sessions' AND column_name = 'created_ts') THEN
            ALTER TABLE saml_sessions RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed saml_sessions.created_at to created_ts';
        END IF;
    END IF;

    -- 4. scheduled_notifications 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'scheduled_notifications' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'scheduled_notifications' AND column_name = 'created_ts') THEN
            ALTER TABLE scheduled_notifications RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed scheduled_notifications.created_at to created_ts';
        END IF;
    END IF;

    -- 5. security_events 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'security_events' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'security_events' AND column_name = 'created_ts') THEN
            ALTER TABLE security_events RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed security_events.created_at to created_ts';
        END IF;
    END IF;

    -- 6. server_media_quota 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_media_quota' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_media_quota' AND column_name = 'created_ts') THEN
            ALTER TABLE server_media_quota RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed server_media_quota.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_media_quota' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_media_quota' AND column_name = 'updated_ts') THEN
            ALTER TABLE server_media_quota RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed server_media_quota.updated_at to updated_ts';
        END IF;
    END IF;

    -- 7. server_notifications 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'created_ts') THEN
            ALTER TABLE server_notifications RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed server_notifications.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'server_notifications' AND column_name = 'updated_ts') THEN
            ALTER TABLE server_notifications RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed server_notifications.updated_at to updated_ts';
        END IF;
    END IF;

    -- 8. user_media_quota 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'created_ts') THEN
            ALTER TABLE user_media_quota RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed user_media_quota.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'updated_ts') THEN
            ALTER TABLE user_media_quota RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed user_media_quota.updated_at to updated_ts';
        END IF;
    END IF;

    -- 9. user_profiles 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_profiles' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_profiles' AND column_name = 'updated_ts') THEN
            ALTER TABLE user_profiles RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed user_profiles.updated_at to updated_ts';
        END IF;
    END IF;

    RAISE NOTICE 'Timestamp column naming standardization (Phase 3) completed';
END $$;
