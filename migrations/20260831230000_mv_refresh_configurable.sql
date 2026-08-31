-- P1-6: MV refresh interval 可配置化
--
-- pg_cron 的 cron.schedule() 第二个参数（cron 表达式）必须是常量，无法动态读取配置。
-- 因此本迁移提供一个可重入的管理函数，运维在部署后通过 SQL 调用即可修改刷新间隔。
--
-- 默认值：'*/5 * * * *'（每 5 分钟刷新，与原硬编码行为一致）
--
-- 使用方式（部署后执行）：
--   SELECT configure_rooms_summaries_refresh('*/10 * * * *');  -- 每 10 分钟
--   SELECT configure_rooms_summaries_refresh('*/5 * * * *');  -- 恢复默认
--   SELECT get_current_refresh_schedule();                     -- 查询当前配置
--
-- 撤销：重新运行即可覆盖，或手动 DELETE FROM cron.job WHERE jobname = 'refresh-rooms-summaries'
--

-- 1. 可重入配置函数（与 pg_cron 可用性解耦，始终创建）
CREATE OR REPLACE FUNCTION configure_rooms_summaries_refresh(refresh_interval TEXT)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    IF refresh_interval IS NULL OR refresh_interval = '' THEN
        RAISE EXCEPTION 'refresh_interval cannot be null or empty';
    END IF;

    -- 移除旧调度（pg_cron 不可用时 cron schema 不存在，抛错后本事务回滚）
    PERFORM cron.unschedule('refresh-rooms-summaries');

    -- 插入新调度
    PERFORM cron.schedule(
        'refresh-rooms-summaries',
        refresh_interval,
        'REFRESH MATERIALIZED VIEW CONCURRENTLY rooms_summaries_mv'
    );

    RAISE NOTICE 'rooms_summaries_mv refresh interval updated to: %', refresh_interval;
END;
$$;

-- 2. 查询函数（pg_cron 可用时返回 cron.job 数据；不可用时返回空结果）
CREATE OR REPLACE FUNCTION get_current_refresh_schedule()
RETURNS TABLE(
    mv_name    TEXT,
    schedule   TEXT,
    active     BOOLEAN,
    command    TEXT
)
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    -- pg_cron 不可用时返回空集（避免硬性依赖）
    IF NOT EXISTS (
        SELECT 1 FROM pg_namespace n
        JOIN pg_proc p ON p.pronamespace = n.oid
        WHERE n.nspname = 'cron' AND p.proname = 'unschedule'
    ) THEN
        RETURN;
    END IF;

    RETURN QUERY
        SELECT
            j.jobname::TEXT,
            j.schedule::TEXT,
            j.active,
            j.command::TEXT
        FROM cron.job j
        WHERE j.jobname IN ('refresh-rooms-summaries', 'refresh-public-room-directory');
END;
$$;

-- 3. pg_cron 调度（可重入：先删后插）
-- pg_cron 不可用时跳过（DO 块内无事务，不会阻止函数/视图创建）
DO $$
BEGIN
    -- 移除旧调度（可重入）
    PERFORM cron.unschedule('refresh-rooms-summaries');
    PERFORM cron.unschedule('refresh-public-room-directory');

    -- 插入新调度
    PERFORM cron.schedule(
        'refresh-rooms-summaries',
        '*/5 * * * *',
        'REFRESH MATERIALIZED VIEW CONCURRENTLY rooms_summaries_mv'
    );
    PERFORM cron.schedule(
        'refresh-public-room-directory',
        '*/10 * * * *',
        'REFRESH MATERIALIZED VIEW CONCURRENTLY public_room_directory'
    );

    RAISE NOTICE 'MV refresh schedules configured (rooms_summaries_mv: */5, public_room_directory: */10)';
EXCEPTION
    WHEN undefined_table OR undefined_function THEN
        RAISE NOTICE 'pg_cron extension not available, MV refresh schedules not created (functions still available)';
END $$;

-- 4. 便捷管理视图（pg_cron 可用时才创建）
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'cron') THEN
        EXECUTE 'CREATE OR REPLACE VIEW mv_refresh_config AS
            SELECT
                j.jobid,
                j.jobname,
                j.schedule,
                j.command,
                j.nodename  AS node,
                j.database  AS db,
                j.active
            FROM cron.job j
            WHERE j.jobname IN (''refresh-rooms-summaries'', ''refresh-public-room-directory'')';
        RAISE NOTICE 'mv_refresh_config view created';
    ELSE
        RAISE NOTICE 'pg_cron not available, skipping mv_refresh_config view';
    END IF;
END $$;
