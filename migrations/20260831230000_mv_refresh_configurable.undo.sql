-- P1-6 撤销脚本：恢复硬编码行为
--
-- 撤销策略：删除管理函数、视图、调度；schema baseline 默认硬编码值
-- 若需重新启用，运行本撤销后再次运行正向迁移

DO $$
BEGIN
    PERFORM cron.unschedule('refresh-rooms-summaries');
    PERFORM cron.unschedule('refresh-public-room-directory');
EXCEPTION
    WHEN undefined_table OR undefined_function THEN
        RAISE NOTICE 'pg_cron extension not available, skipping unschedule';
END $$;

-- 删除管理函数和视图
DROP FUNCTION IF EXISTS configure_rooms_summaries_refresh(TEXT);
DROP FUNCTION IF EXISTS get_current_refresh_schedule();
DROP VIEW IF EXISTS mv_refresh_config;