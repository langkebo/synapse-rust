-- =============================================================================
-- worker_statistics 负载指标列回滚
-- =============================================================================

ALTER TABLE worker_statistics
    DROP COLUMN IF EXISTS worker_name,
    DROP COLUMN IF EXISTS worker_type,
    DROP COLUMN IF EXISTS status,
    DROP COLUMN IF EXISTS host,
    DROP COLUMN IF EXISTS port,
    DROP COLUMN IF EXISTS last_heartbeat_ts,
    DROP COLUMN IF EXISTS started_ts,
    DROP COLUMN IF EXISTS cpu_usage,
    DROP COLUMN IF EXISTS memory_usage,
    DROP COLUMN IF EXISTS active_connections,
    DROP COLUMN IF EXISTS requests_per_second,
    DROP COLUMN IF EXISTS average_latency_ms,
    DROP COLUMN IF EXISTS queue_depth,
    DROP COLUMN IF EXISTS pending_commands,
    DROP COLUMN IF EXISTS active_tasks;
