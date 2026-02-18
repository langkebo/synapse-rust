-- 消息保留策略表
-- 支持自动清理过期消息，满足合规和存储管理需求

-- 房间保留策略表
CREATE TABLE IF NOT EXISTS room_retention_policies (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    max_lifetime BIGINT,
    min_lifetime BIGINT DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    is_server_default BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_retention_policies_room_id ON room_retention_policies(room_id);
CREATE INDEX IF NOT EXISTS idx_room_retention_policies_max_lifetime ON room_retention_policies(max_lifetime);
CREATE INDEX IF NOT EXISTS idx_room_retention_policies_expire_on_clients ON room_retention_policies(expire_on_clients);

-- 服务器默认保留策略表
CREATE TABLE IF NOT EXISTS server_retention_policy (
    id BIGSERIAL PRIMARY KEY,
    max_lifetime BIGINT,
    min_lifetime BIGINT DEFAULT 0,
    expire_on_clients BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

INSERT INTO server_retention_policy (max_lifetime, min_lifetime, expire_on_clients, created_ts, updated_ts)
VALUES (NULL, 0, FALSE, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000)
ON CONFLICT DO NOTHING;

-- 保留策略清理队列表
CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    event_type VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    scheduled_ts BIGINT NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_room_id ON retention_cleanup_queue(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_status ON retention_cleanup_queue(status);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_scheduled_ts ON retention_cleanup_queue(scheduled_ts);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_queue_origin_ts ON retention_cleanup_queue(origin_server_ts);

-- 保留策略清理日志表
CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    events_deleted BIGINT DEFAULT 0,
    state_events_deleted BIGINT DEFAULT 0,
    media_deleted BIGINT DEFAULT 0,
    bytes_freed BIGINT DEFAULT 0,
    started_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    status VARCHAR(50) DEFAULT 'running',
    error_message TEXT,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_room_id ON retention_cleanup_logs(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_started_ts ON retention_cleanup_logs(started_ts DESC);
CREATE INDEX IF NOT EXISTS idx_retention_cleanup_logs_status ON retention_cleanup_logs(status);

-- 已删除事件索引表（用于联邦通知）
CREATE TABLE IF NOT EXISTS deleted_events_index (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    deletion_ts BIGINT NOT NULL,
    reason VARCHAR(50) DEFAULT 'retention',
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_deleted_events_index_room_id ON deleted_events_index(room_id);
CREATE INDEX IF NOT EXISTS idx_deleted_events_index_event_id ON deleted_events_index(event_id);
CREATE INDEX IF NOT EXISTS idx_deleted_events_index_deletion_ts ON deleted_events_index(deletion_ts DESC);

-- 保留策略统计表
CREATE TABLE IF NOT EXISTS retention_stats (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    total_events BIGINT DEFAULT 0,
    events_in_retention BIGINT DEFAULT 0,
    events_expired BIGINT DEFAULT 0,
    last_cleanup_ts BIGINT,
    next_cleanup_ts BIGINT,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_retention_stats_room_id ON retention_stats(room_id);
CREATE INDEX IF NOT EXISTS idx_retention_stats_next_cleanup ON retention_stats(next_cleanup_ts);

-- 触发器：自动更新 updated_ts
CREATE OR REPLACE FUNCTION update_retention_policy_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_room_retention_policy_timestamp
    BEFORE UPDATE ON room_retention_policies
    FOR EACH ROW
    EXECUTE FUNCTION update_retention_policy_timestamp();

CREATE TRIGGER trigger_update_server_retention_policy_timestamp
    BEFORE UPDATE ON server_retention_policy
    FOR EACH ROW
    EXECUTE FUNCTION update_retention_policy_timestamp();

-- 函数：获取房间有效保留策略
CREATE OR REPLACE FUNCTION get_effective_retention_policy(p_room_id VARCHAR)
RETURNS TABLE (
    max_lifetime BIGINT,
    min_lifetime BIGINT,
    expire_on_clients BOOLEAN
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        COALESCE(rrp.max_lifetime, srp.max_lifetime) as max_lifetime,
        COALESCE(rrp.min_lifetime, srp.min_lifetime) as min_lifetime,
        COALESCE(rrp.expire_on_clients, srp.expire_on_clients) as expire_on_clients
    FROM server_retention_policy srp
    LEFT JOIN room_retention_policies rrp ON rrp.room_id = p_room_id
    LIMIT 1;
END;
$$ LANGUAGE plpgsql;

-- 函数：计算事件是否过期
CREATE OR REPLACE FUNCTION is_event_expired(
    p_room_id VARCHAR,
    p_origin_server_ts BIGINT
) RETURNS BOOLEAN AS $$
DECLARE
    v_max_lifetime BIGINT;
    v_cutoff_ts BIGINT;
BEGIN
    SELECT max_lifetime INTO v_max_lifetime
    FROM get_effective_retention_policy(p_room_id);
    
    IF v_max_lifetime IS NULL THEN
        RETURN FALSE;
    END IF;
    
    v_cutoff_ts := (EXTRACT(EPOCH FROM NOW()) * 1000) - v_max_lifetime;
    
    RETURN p_origin_server_ts < v_cutoff_ts;
END;
$$ LANGUAGE plpgsql;

-- 函数：调度过期事件清理
CREATE OR REPLACE FUNCTION schedule_retention_cleanup(p_room_id VARCHAR)
RETURNS VOID AS $$
DECLARE
    v_max_lifetime BIGINT;
    v_cutoff_ts BIGINT;
    v_event RECORD;
BEGIN
    SELECT max_lifetime INTO v_max_lifetime
    FROM get_effective_retention_policy(p_room_id);
    
    IF v_max_lifetime IS NULL THEN
        RETURN;
    END IF;
    
    v_cutoff_ts := (EXTRACT(EPOCH FROM NOW()) * 1000) - v_max_lifetime;
    
    FOR v_event IN 
        SELECT event_id, event_type, origin_server_ts
        FROM events 
        WHERE room_id = p_room_id 
        AND origin_server_ts < v_cutoff_ts
        AND event_type NOT IN ('m.room.create', 'm.room.power_levels', 'm.room.join_rules', 'm.room.history_visibility')
        AND state_key IS NULL
    LOOP
        INSERT INTO retention_cleanup_queue (room_id, event_id, event_type, origin_server_ts, scheduled_ts, created_ts)
        VALUES (p_room_id, v_event.event_id, v_event.event_type, v_event.origin_server_ts, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000)
        ON CONFLICT DO NOTHING;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- 视图：待清理事件统计
CREATE OR REPLACE VIEW v_retention_cleanup_stats AS
SELECT 
    rcq.room_id,
    COUNT(*) as pending_events,
    MIN(rcq.origin_server_ts) as oldest_event_ts,
    MAX(rcq.origin_server_ts) as newest_event_ts
FROM retention_cleanup_queue rcq
WHERE rcq.status = 'pending'
GROUP BY rcq.room_id;

-- 视图：房间保留状态
CREATE OR REPLACE VIEW v_room_retention_status AS
SELECT 
    r.room_id,
    rrp.max_lifetime as room_max_lifetime,
    srp.max_lifetime as server_max_lifetime,
    COALESCE(rrp.max_lifetime, srp.max_lifetime) as effective_max_lifetime,
    rs.total_events,
    rs.events_expired,
    rs.last_cleanup_ts,
    rs.next_cleanup_ts
FROM rooms r
LEFT JOIN room_retention_policies rrp ON rrp.room_id = r.room_id
CROSS JOIN server_retention_policy srp
LEFT JOIN retention_stats rs ON rs.room_id = r.room_id;
