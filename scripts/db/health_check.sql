-- ============================================================================
-- 数据库健康检测脚本: /scripts/db/health_check.sql
-- 用途: 定期检测 PostgreSQL 数据库健康状态
-- 使用: psql -h <host> -U <user> -d <dbname> -f scripts/db/health_check.sql
-- 更新: 2026-05-09
-- ============================================================================

\echo '========================================'
\echo '  synapse-rust 数据库健康检测报告'
\echo '  运行时间:'
\echo '========================================'
SELECT NOW() AS check_time;

-- ============================================================================
-- 1. 连接池与活跃连接
-- ============================================================================
\echo ''
\echo '=== 1. 连接状态 ==='
SELECT
    state,
    COUNT(*) AS connections,
    ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER(), 1) AS pct
FROM pg_stat_activity
WHERE backend_type = 'client backend'
GROUP BY state
ORDER BY state;

\echo ''
\echo '-- 长事务检测 (> 5 分钟) --'
SELECT
    pid,
    usename,
    application_name,
    state,
    NOW() - xact_start AS xact_duration,
    NOW() - query_start AS query_duration,
    LEFT(query, 80) AS query_preview
FROM pg_stat_activity
WHERE state != 'idle'
  AND xact_start IS NOT NULL
  AND NOW() - xact_start > interval '5 minutes'
ORDER BY xact_start;

-- ============================================================================
-- 2. 数据库大小与表膨胀
-- ============================================================================
\echo ''
\echo '=== 2. 表大小统计 (Top 20) ==='
SELECT
    schemaname || '.' || relname AS table_name,
    pg_size_pretty(pg_total_relation_size(relid)) AS total_size,
    pg_size_pretty(pg_relation_size(relid)) AS table_size,
    pg_size_pretty(pg_total_relation_size(relid) - pg_relation_size(relid)) AS index_size,
    n_live_tup AS estimated_rows,
    n_dead_tup AS dead_tuples,
    CASE
        WHEN n_live_tup > 0
        THEN ROUND(n_dead_tup * 100.0 / (n_live_tup + n_dead_tup), 1)
        ELSE 0
    END AS dead_tuple_pct,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
FROM pg_stat_user_tables
JOIN (
    SELECT
        c.oid AS relid,
        n.nspname AS schemaname,
        c.relname
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
) t ON t.relid = relid
ORDER BY pg_total_relation_size(relid) DESC
LIMIT 20;

-- ============================================================================
-- 3. 索引审计
-- ============================================================================
\echo ''
\echo '=== 3.1 索引使用统计 (Top 15 未使用索引) ==='
SELECT
    schemaname || '.' || relname AS table_name,
    indexrelname AS index_name,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size,
    idx_scan AS index_scans_since_reset,
    idx_tup_read AS tuples_returned,
    idx_tup_fetch AS tuples_fetched
FROM pg_stat_user_indexes
WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
  AND idx_scan < 10
  AND pg_relation_size(indexrelid) > 8192  -- 忽略极小索引
ORDER BY pg_relation_size(indexrelid) DESC
LIMIT 15;

\echo ''
\echo '=== 3.2 重复索引检测 ==='
SELECT
    a.schemaname || '.' || a.relname AS table_name,
    a.indexrelname AS index_1,
    b.indexrelname AS index_2,
    pg_size_pretty(pg_relation_size(a.indexrelid)) AS size_1,
    pg_size_pretty(pg_relation_size(b.indexrelid)) AS size_2
FROM pg_stat_user_indexes a
JOIN pg_stat_user_indexes b
    ON a.relid = b.relid
    AND a.indexrelid < b.indexrelid
JOIN pg_index ai ON a.indexrelid = ai.indexrelid
JOIN pg_index bi ON b.indexrelid = bi.indexrelid
WHERE a.schemaname NOT IN ('pg_catalog', 'information_schema')
  AND ai.indkey = bi.indkey
  AND ai.indoption = bi.indoption
  AND ai.indpred IS NOT DISTINCT FROM bi.indpred
ORDER BY pg_relation_size(a.indexrelid) + pg_relation_size(b.indexrelid) DESC;

\echo ''
\echo '=== 3.3 缺失索引检测（顺序扫描 > 1000 次的表） ==='
SELECT
    schemaname || '.' || relname AS table_name,
    seq_scan,
    seq_tup_read,
    idx_scan,
    n_live_tup AS estimated_rows,
    pg_size_pretty(pg_total_relation_size(relid)) AS total_size
FROM pg_stat_user_tables
JOIN (
    SELECT c.oid AS relid, n.nspname AS schemaname, c.relname
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
) t ON t.relid = relid
WHERE seq_scan > 1000
  AND n_live_tup > 1000
ORDER BY seq_scan DESC
LIMIT 15;

-- ============================================================================
-- 4. 序列空洞检测
-- ============================================================================
\echo ''
\echo '=== 4. 序列健康检测 ==='
SELECT
    schemaname || '.' || sequencename AS sequence_name,
    start_value,
    min_value,
    max_value,
    increment_by,
    last_value,
    CASE
        WHEN max_value > 0
        THEN ROUND(last_value * 100.0 / max_value, 2)
        ELSE 0
    END AS usage_pct,
    is_cycled
FROM pg_sequences
WHERE sequencename LIKE '%_id_seq'
   OR sequencename LIKE '%stream%'
   OR sequencename LIKE '%ordering%'
ORDER BY usage_pct DESC;

-- ============================================================================
-- 5. 死锁记录 (最近 24 小时)
-- ============================================================================
\echo ''
\echo '=== 5. 死锁检测 (最近 24 小时) ==='
SELECT
    deadlocks,
    xact_commit,
    xact_rollback,
    CASE
        WHEN xact_commit + xact_rollback > 0
        THEN ROUND(xact_rollback * 100.0 / (xact_commit + xact_rollback), 3)
        ELSE 0
    END AS rollback_ratio_pct,
    blks_hit,
    blks_read,
    CASE
        WHEN blks_read > 0
        THEN ROUND(blks_hit * 100.0 / (blks_hit + blks_read), 2)
        ELSE 100
    END AS cache_hit_ratio_pct
FROM pg_stat_database
WHERE datname = current_database();

-- ============================================================================
-- 6. Vacuum 状态
-- ============================================================================
\echo ''
\echo '=== 6. Vacuum 状态 ==='
SELECT
    schemaname || '.' || relname AS table_name,
    n_live_tup,
    n_dead_tup,
    n_mod_since_analyze,
    last_vacuum,
    last_autovacuum,
    autovacuum_count,
    vacuum_count,
    CASE
        WHEN last_autovacuum IS NOT NULL
        THEN EXTRACT(EPOCH FROM NOW() - last_autovacuum) / 3600
        ELSE NULL
    END AS hours_since_last_autovacuum
FROM pg_stat_user_tables
WHERE n_dead_tup > n_live_tup * 0.1  -- dead tuples > 10% of live
  AND n_live_tup > 1000
ORDER BY n_dead_tup DESC
LIMIT 15;

-- ============================================================================
-- 7. 行级安全与缺少主键的表
-- ============================================================================
\echo ''
\echo '=== 7. 缺少主键的表 ==='
SELECT
    n.nspname || '.' || c.relname AS table_name,
    pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size,
    c.reltuples::bigint AS estimated_rows
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_constraint con
    ON con.conrelid = c.oid AND con.contype = 'p'
WHERE c.relkind = 'r'
  AND n.nspname NOT IN ('pg_catalog', 'information_schema')
  AND con.conname IS NULL
ORDER BY pg_total_relation_size(c.oid) DESC;

-- ============================================================================
-- 8. 总结
-- ============================================================================
\echo ''
\echo '========================================'
\echo '  检测完成'
\echo '========================================'
\echo '  - 死锁率 > 1% 需关注'
\echo '  - 缓存命中率 < 95% 建议增加 shared_buffers'
\echo '  - dead_tuple_pct > 20% 建议手动 VACUUM FULL'
\echo '  - hours_since_last_autovacuum > 24 需检查 autovacuum 配置'
