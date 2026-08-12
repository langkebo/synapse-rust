-- =============================================================================
-- worker_statistics 补齐实时负载指标列
-- =============================================================================
-- 背景: worker/v1/statistics 接口的 SQL 查询引用了 15 个在
--   worker_statistics 表中不存在的列 (worker_name, cpu_usage 等)，
--   导致接口返回 500 "column ... does not exist"。
-- 本迁移补齐这些列，使统计接口按设计从 worker_statistics 读取数据。
-- 实时负载指标 (cpu/memory/connections/...) 暂无采集器，保持 NULL；
-- 后续接入负载采集后直接 UPDATE 对应列即可，无需再改接口。
-- =============================================================================

ALTER TABLE worker_statistics
    ADD COLUMN IF NOT EXISTS worker_name TEXT,
    ADD COLUMN IF NOT EXISTS worker_type TEXT,
    ADD COLUMN IF NOT EXISTS status TEXT,
    ADD COLUMN IF NOT EXISTS host TEXT,
    ADD COLUMN IF NOT EXISTS port INTEGER,
    ADD COLUMN IF NOT EXISTS last_heartbeat_ts BIGINT,
    ADD COLUMN IF NOT EXISTS started_ts BIGINT,
    ADD COLUMN IF NOT EXISTS cpu_usage DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS memory_usage DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS active_connections INTEGER,
    ADD COLUMN IF NOT EXISTS requests_per_second DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS average_latency_ms DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS queue_depth INTEGER,
    ADD COLUMN IF NOT EXISTS pending_commands INTEGER,
    ADD COLUMN IF NOT EXISTS active_tasks INTEGER;

-- 从 workers 表回填身份/生命周期字段，保证存量统计行立即可读
UPDATE worker_statistics ws
   SET worker_name       = w.worker_name,
       worker_type       = w.worker_type,
       status            = w.status,
       host              = w.host,
       port              = w.port,
       last_heartbeat_ts = w.last_heartbeat_ts,
       started_ts        = w.started_ts
  FROM workers w
 WHERE ws.worker_id = w.worker_id;
