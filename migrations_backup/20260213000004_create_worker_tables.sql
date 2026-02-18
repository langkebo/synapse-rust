-- =============================================================================
-- Synapse-Rust Worker 架构数据库迁移脚本
-- 版本: 1.0
-- 创建日期: 2026-02-13
-- PostgreSQL版本: 15.x 兼容
-- 描述: 实现 Worker 架构，支持水平扩展和负载均衡
-- =============================================================================

-- Worker 注册表: 存储已注册的 Worker 实例
CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL UNIQUE,
    worker_name VARCHAR(255) NOT NULL,
    worker_type VARCHAR(50) NOT NULL,
    host VARCHAR(255) NOT NULL,
    port INTEGER NOT NULL,
    status VARCHAR(50) DEFAULT 'starting',
    last_heartbeat_ts BIGINT,
    started_ts BIGINT NOT NULL,
    stopped_ts BIGINT,
    config JSONB DEFAULT '{}'::jsonb,
    metadata JSONB DEFAULT '{}'::jsonb,
    version VARCHAR(50),
    CHECK (worker_type IN ('master', 'frontend', 'background', 'event_persister', 'synchrotron', 'federation_sender', 'federation_reader', 'media_repository', 'pusher', 'appservice'))
);

-- Worker 索引
CREATE INDEX IF NOT EXISTS idx_workers_worker_id ON workers(worker_id);
CREATE INDEX IF NOT EXISTS idx_workers_type ON workers(worker_type);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_heartbeat ON workers(last_heartbeat_ts) WHERE status = 'running';

-- Worker 命令队列表: 存储发送给 Worker 的命令
CREATE TABLE IF NOT EXISTS worker_commands (
    id BIGSERIAL PRIMARY KEY,
    command_id VARCHAR(255) NOT NULL UNIQUE,
    target_worker_id VARCHAR(255) NOT NULL,
    source_worker_id VARCHAR(255),
    command_type VARCHAR(100) NOT NULL,
    command_data JSONB DEFAULT '{}'::jsonb,
    priority INTEGER DEFAULT 0,
    status VARCHAR(50) DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    sent_ts BIGINT,
    completed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    FOREIGN KEY (target_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
);

-- Worker 命令索引
CREATE INDEX IF NOT EXISTS idx_worker_commands_target ON worker_commands(target_worker_id, status);
CREATE INDEX IF NOT EXISTS idx_worker_commands_status ON worker_commands(status, priority DESC, created_ts);
CREATE INDEX IF NOT EXISTS idx_worker_commands_type ON worker_commands(command_type);

-- Worker 事件流表: 存储需要复制的事件
CREATE TABLE IF NOT EXISTS worker_events (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    stream_id BIGINT NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    room_id VARCHAR(255),
    sender VARCHAR(255),
    event_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_ts BIGINT NOT NULL,
    processed_by TEXT[],
    UNIQUE(event_id)
);

-- Worker 事件索引
CREATE INDEX IF NOT EXISTS idx_worker_events_stream ON worker_events(stream_id);
CREATE INDEX IF NOT EXISTS idx_worker_events_type ON worker_events(event_type);
CREATE INDEX IF NOT EXISTS idx_worker_events_room ON worker_events(room_id);
CREATE INDEX IF NOT EXISTS idx_worker_events_ts ON worker_events(created_ts DESC);

-- 复制流位置表: 跟踪每个 Worker 的复制进度
CREATE TABLE IF NOT EXISTS replication_positions (
    id BIGSERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    stream_name VARCHAR(100) NOT NULL,
    stream_position BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(worker_id, stream_name),
    FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
);

-- 复制流位置索引
CREATE INDEX IF NOT EXISTS idx_replication_positions_worker ON replication_positions(worker_id);
CREATE INDEX IF NOT EXISTS idx_replication_positions_stream ON replication_positions(stream_name);

-- Worker 健康检查表: 记录 Worker 健康状态
CREATE TABLE IF NOT EXISTS worker_health_checks (
    id BIGSERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    check_type VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    details JSONB DEFAULT '{}'::jsonb,
    checked_ts BIGINT NOT NULL,
    FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
);

-- Worker 健康检查索引
CREATE INDEX IF NOT EXISTS idx_worker_health_worker ON worker_health_checks(worker_id, checked_ts DESC);
CREATE INDEX IF NOT EXISTS idx_worker_health_status ON worker_health_checks(status);

-- Worker 负载统计表: 记录 Worker 负载信息
CREATE TABLE IF NOT EXISTS worker_load_stats (
    id BIGSERIAL PRIMARY KEY,
    worker_id VARCHAR(255) NOT NULL,
    cpu_usage REAL,
    memory_usage BIGINT,
    active_connections INTEGER,
    requests_per_second REAL,
    average_latency_ms REAL,
    queue_depth INTEGER,
    recorded_ts BIGINT NOT NULL,
    FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
);

-- Worker 负载统计索引
CREATE INDEX IF NOT EXISTS idx_worker_load_worker ON worker_load_stats(worker_id, recorded_ts DESC);

-- Worker 任务分配表: 跟踪任务分配情况
CREATE TABLE IF NOT EXISTS worker_task_assignments (
    id BIGSERIAL PRIMARY KEY,
    task_id VARCHAR(255) NOT NULL UNIQUE,
    task_type VARCHAR(100) NOT NULL,
    task_data JSONB DEFAULT '{}'::jsonb,
    assigned_worker_id VARCHAR(255),
    status VARCHAR(50) DEFAULT 'pending',
    priority INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    assigned_ts BIGINT,
    completed_ts BIGINT,
    result JSONB,
    error_message TEXT,
    FOREIGN KEY (assigned_worker_id) REFERENCES workers(worker_id) ON DELETE SET NULL
);

-- Worker 任务分配索引
CREATE INDEX IF NOT EXISTS idx_worker_tasks_type ON worker_task_assignments(task_type, status);
CREATE INDEX IF NOT EXISTS idx_worker_tasks_worker ON worker_task_assignments(assigned_worker_id, status);
CREATE INDEX IF NOT EXISTS idx_worker_tasks_status ON worker_task_assignments(status, priority DESC, created_ts);

-- Worker 连接表: 跟踪 Worker 之间的连接
CREATE TABLE IF NOT EXISTS worker_connections (
    id BIGSERIAL PRIMARY KEY,
    source_worker_id VARCHAR(255) NOT NULL,
    target_worker_id VARCHAR(255) NOT NULL,
    connection_type VARCHAR(50) NOT NULL,
    status VARCHAR(50) DEFAULT 'connected',
    established_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    bytes_sent BIGINT DEFAULT 0,
    bytes_received BIGINT DEFAULT 0,
    messages_sent BIGINT DEFAULT 0,
    messages_received BIGINT DEFAULT 0,
    FOREIGN KEY (source_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE,
    FOREIGN KEY (target_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE,
    UNIQUE(source_worker_id, target_worker_id, connection_type)
);

-- Worker 连接索引
CREATE INDEX IF NOT EXISTS idx_worker_connections_source ON worker_connections(source_worker_id);
CREATE INDEX IF NOT EXISTS idx_worker_connections_target ON worker_connections(target_worker_id);

-- 创建序列用于流 ID 生成
CREATE SEQUENCE IF NOT EXISTS worker_event_stream_id_seq START 1;

-- 创建更新心跳触发器函数
CREATE OR REPLACE FUNCTION update_worker_heartbeat()
RETURNS TRIGGER AS $$
BEGIN
    NEW.last_heartbeat_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 添加注释
COMMENT ON TABLE workers IS 'Worker 注册表，存储所有 Worker 实例信息';
COMMENT ON TABLE worker_commands IS 'Worker 命令队列，存储发送给 Worker 的命令';
COMMENT ON TABLE worker_events IS 'Worker 事件流，存储需要复制的事件';
COMMENT ON TABLE replication_positions IS '复制流位置，跟踪每个 Worker 的复制进度';
COMMENT ON TABLE worker_health_checks IS 'Worker 健康检查记录';
COMMENT ON TABLE worker_load_stats IS 'Worker 负载统计';
COMMENT ON TABLE worker_task_assignments IS 'Worker 任务分配表';
COMMENT ON TABLE worker_connections IS 'Worker 连接表，跟踪 Worker 之间的连接';

-- 创建 Worker 统计视图
CREATE OR REPLACE VIEW worker_statistics AS
SELECT 
    w.id,
    w.worker_id,
    w.worker_name,
    w.worker_type,
    w.status,
    w.host,
    w.port,
    w.last_heartbeat_ts,
    w.started_ts,
    wls.cpu_usage,
    wls.memory_usage,
    wls.active_connections,
    wls.requests_per_second,
    wls.average_latency_ms,
    wls.queue_depth,
    (SELECT COUNT(*) FROM worker_commands WHERE target_worker_id = w.worker_id AND status = 'pending') AS pending_commands,
    (SELECT COUNT(*) FROM worker_task_assignments WHERE assigned_worker_id = w.worker_id AND status IN ('pending', 'running')) AS active_tasks
FROM workers w
LEFT JOIN LATERAL (
    SELECT cpu_usage, memory_usage, active_connections, requests_per_second, average_latency_ms, queue_depth
    FROM worker_load_stats wls
    WHERE wls.worker_id = w.worker_id
    ORDER BY wls.recorded_ts DESC
    LIMIT 1
) wls ON true;

COMMENT ON VIEW worker_statistics IS 'Worker 统计视图，提供 Worker 的综合统计信息';

-- 创建活跃 Worker 视图
CREATE OR REPLACE VIEW active_workers AS
SELECT 
    worker_id,
    worker_name,
    worker_type,
    host,
    port,
    status,
    last_heartbeat_ts,
    started_ts,
    version
FROM workers
WHERE status = 'running'
  AND (last_heartbeat_ts IS NULL OR last_heartbeat_ts > EXTRACT(EPOCH FROM NOW()) * 1000 - 60000);

COMMENT ON VIEW active_workers IS '活跃 Worker 视图，显示最近 60 秒内有心跳的 Worker';

-- 创建 Worker 类型统计视图
CREATE OR REPLACE VIEW worker_type_statistics AS
SELECT 
    worker_type,
    COUNT(*) AS total_count,
    COUNT(*) FILTER (WHERE status = 'running') AS running_count,
    COUNT(*) FILTER (WHERE status = 'starting') AS starting_count,
    COUNT(*) FILTER (WHERE status = 'stopping') AS stopping_count,
    COUNT(*) FILTER (WHERE status = 'stopped') AS stopped_count,
    AVG(wls.cpu_usage) AS avg_cpu_usage,
    AVG(wls.memory_usage) AS avg_memory_usage,
    SUM(wls.active_connections) AS total_connections
FROM workers
LEFT JOIN LATERAL (
    SELECT cpu_usage, memory_usage, active_connections
    FROM worker_load_stats wls
    WHERE wls.worker_id = workers.worker_id
    ORDER BY wls.recorded_ts DESC
    LIMIT 1
) wls ON true
GROUP BY worker_type;

COMMENT ON VIEW worker_type_statistics IS 'Worker 类型统计视图，按类型汇总 Worker 信息';
