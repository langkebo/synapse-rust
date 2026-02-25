-- 统一时间戳列命名规范迁移脚本
-- 规范：
--   - _ts 后缀：毫秒级 Unix 时间戳 (BIGINT)
--   - _at 后缀：TIMESTAMPTZ 时间类型
-- 
-- 本项目统一使用 _ts 后缀存储毫秒级时间戳

DO $$
DECLARE
    rec RECORD;
BEGIN
    -- 1. 统一 federation_signing_keys 表
    -- created_at -> created_ts (如果存在 created_at 且不存在 created_ts)
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_signing_keys' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'federation_signing_keys' AND column_name = 'created_ts') THEN
            ALTER TABLE federation_signing_keys RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed federation_signing_keys.created_at to created_ts';
        END IF;
    END IF;

    -- 2. 统一 presence 表
    -- updated_at -> updated_ts
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'presence' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'presence' AND column_name = 'updated_ts') THEN
            ALTER TABLE presence RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed presence.updated_at to updated_ts';
        END IF;
    END IF;

    -- 3. 统一 notifications 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'notifications' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'notifications' AND column_name = 'created_ts') THEN
            ALTER TABLE notifications RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed notifications.created_at to created_ts';
        END IF;
    END IF;

    -- 4. 统一 account_data 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data' AND column_name = 'created_ts') THEN
            ALTER TABLE account_data RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed account_data.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data' AND column_name = 'updated_ts') THEN
            ALTER TABLE account_data RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed account_data.updated_at to updated_ts';
        END IF;
    END IF;

    -- 5. 统一 room_account_data 表
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_account_data' AND column_name = 'created_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_account_data' AND column_name = 'created_ts') THEN
            ALTER TABLE room_account_data RENAME COLUMN created_at TO created_ts;
            RAISE NOTICE 'Renamed room_account_data.created_at to created_ts';
        END IF;
    END IF;
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_account_data' AND column_name = 'updated_at') THEN
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_account_data' AND column_name = 'updated_ts') THEN
            ALTER TABLE room_account_data RENAME COLUMN updated_at TO updated_ts;
            RAISE NOTICE 'Renamed room_account_data.updated_at to updated_ts';
        END IF;
    END IF;

    RAISE NOTICE 'Timestamp column naming standardization completed';
END $$;
