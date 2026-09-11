-- =====================================================================
-- Migration: 20260909010000_drop_stale_legacy_tables.sql
-- 目的:
--   收敛 pre-v11 升级库中遗留的废弃表，消除 schema 健康检查的
--   baseline drift 警告（baseline 期望 253 张表，旧库实际存在 264 个
--   public 对象）。以下表在历史版本中已被明确移除，仅因旧库在迁移文件
--   折叠进 baseline 前未执行 DROP 而残留：
--     - worker_connections / worker_load_stats: v8 已 DROP
--       （v11 baseline 第 4110 行注释；db_schema_smoke_tests_migrated.rs:482）
--     - retention_cleanup_logs / retention_cleanup_queue / retention_stats /
--       deleted_events_index: v10 已 DROP
--       （db_schema_smoke_tests_migrated.rs:246）
--     - room_children: v10 被 space_children 取代
--       （db_schema_smoke_tests_migrated.rs:274）
--   依据: 生产存储层已改用 worker_statistics / space_children 等现行表，
--   上述表均无应用代码引用且表中 0 行；全新库从 baseline 初始化时本就不
--   含这些表，故此迁移在全新库上为幂等空操作。
--   注意: 老库中的 worker_type_statistics 视图仍引用已废弃的
--   worker_connections / worker_load_stats（v8 前定义），导致直接 DROP
--   失败（历史告警: cannot drop table worker_connections because other
--   objects depend on it）。此处先将其重建为 v11 baseline 的 canonical
--   定义（仅读 workers，聚合列恒为 NULL/0，见 baseline 第 4109-4124 行
--   注释）；同时所有 DROP 均使用 CASCADE，以确保老库中任何其它历史
--   依赖对象（视图/约束/规则）不会再次阻塞删除——这些表本身无任何
--   现行代码引用，级联删除不会波及 canonical schema。
-- =====================================================================

-- 1) 重建 worker_type_statistics 为 canonical 定义，解除对废弃表的依赖
CREATE OR REPLACE VIEW worker_type_statistics AS
SELECT
    w.worker_type,
    COUNT(*)::BIGINT AS total_count,
    COUNT(*) FILTER (WHERE w.status = 'running')::BIGINT AS running_count,
    COUNT(*) FILTER (WHERE w.status = 'starting')::BIGINT AS starting_count,
    COUNT(*) FILTER (WHERE w.status = 'stopping')::BIGINT AS stopping_count,
    COUNT(*) FILTER (WHERE w.status = 'stopped')::BIGINT AS stopped_count,
    NULL::DOUBLE PRECISION AS avg_cpu_usage,
    NULL::DOUBLE PRECISION AS avg_memory_usage,
    0::BIGINT AS total_connections
FROM workers w
GROUP BY w.worker_type;

-- 2) 删除 v8/v10 已废弃的残留表
--    全部使用 CASCADE：老库中可能仍存在其它历史依赖对象（视图/约束/规则），
--    不加 CASCADE 会以 "cannot drop table X because other objects depend on it"
--    告警形式失败；这些表无任何现行代码引用，级联删除不会波及 canonical schema。
DROP TABLE IF EXISTS deleted_events_index CASCADE;
DROP TABLE IF EXISTS retention_cleanup_logs CASCADE;
DROP TABLE IF EXISTS retention_cleanup_queue CASCADE;
DROP TABLE IF EXISTS retention_stats CASCADE;
DROP TABLE IF EXISTS room_children CASCADE;
DROP TABLE IF EXISTS worker_connections CASCADE;
DROP TABLE IF EXISTS worker_load_stats CASCADE;
