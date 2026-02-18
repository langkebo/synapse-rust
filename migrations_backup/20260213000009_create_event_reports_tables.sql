-- 事件报告表
-- 支持用户举报不当内容

-- 事件报告主表
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    reporter_user_id VARCHAR(255) NOT NULL,
    reported_user_id VARCHAR(255),
    event_json JSONB,
    reason VARCHAR(255),
    description TEXT,
    status VARCHAR(50) DEFAULT 'open',
    score INTEGER DEFAULT 0,
    received_ts BIGINT NOT NULL,
    resolved_ts BIGINT,
    resolved_by VARCHAR(255),
    resolution_reason TEXT,
    FOREIGN KEY (reporter_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (reported_user_id) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_event_reports_event_id ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room_id ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reported ON event_reports(reported_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_status ON event_reports(status);
CREATE INDEX IF NOT EXISTS idx_event_reports_received_ts ON event_reports(received_ts DESC);
CREATE INDEX IF NOT EXISTS idx_event_reports_reason ON event_reports(reason);

-- 报告处理历史表
CREATE TABLE IF NOT EXISTS event_report_history (
    id BIGSERIAL PRIMARY KEY,
    report_id BIGINT NOT NULL,
    action VARCHAR(255) NOT NULL,
    actor_user_id VARCHAR(255),
    actor_role VARCHAR(50),
    old_status VARCHAR(50),
    new_status VARCHAR(50),
    reason TEXT,
    created_ts BIGINT NOT NULL,
    metadata JSONB,
    FOREIGN KEY (report_id) REFERENCES event_reports(id) ON DELETE CASCADE,
    FOREIGN KEY (actor_user_id) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_event_report_history_report_id ON event_report_history(report_id);
CREATE INDEX IF NOT EXISTS idx_event_report_history_actor ON event_report_history(actor_user_id);
CREATE INDEX IF NOT EXISTS idx_event_report_history_created_ts ON event_report_history(created_ts DESC);

-- 用户举报限制表（防止滥用）
CREATE TABLE IF NOT EXISTS report_rate_limits (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE,
    report_count INTEGER DEFAULT 0,
    last_report_ts BIGINT,
    blocked_until_ts BIGINT,
    is_blocked BOOLEAN DEFAULT FALSE,
    block_reason TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user_id ON report_rate_limits(user_id);
CREATE INDEX IF NOT EXISTS idx_report_rate_limits_blocked ON report_rate_limits(is_blocked);

-- 报告统计表
CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL UNIQUE,
    total_reports INTEGER DEFAULT 0,
    open_reports INTEGER DEFAULT 0,
    resolved_reports INTEGER DEFAULT 0,
    dismissed_reports INTEGER DEFAULT 0,
    avg_resolution_time_ms BIGINT,
    reports_by_reason JSONB,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_event_report_stats_date ON event_report_stats(stat_date DESC);

-- 触发器：自动更新 updated_ts
CREATE OR REPLACE FUNCTION update_report_rate_limits_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_report_rate_limits_timestamp
    BEFORE UPDATE ON report_rate_limits
    FOR EACH ROW
    EXECUTE FUNCTION update_report_rate_limits_timestamp();

-- 函数：检查用户举报限制
CREATE OR REPLACE FUNCTION check_report_rate_limit(p_user_id VARCHAR)
RETURNS TABLE (
    is_allowed BOOLEAN,
    remaining_reports INTEGER,
    block_reason TEXT
) AS $$
DECLARE
    v_limit RECORD;
    v_max_reports_per_day INTEGER := 50;
BEGIN
    SELECT * INTO v_limit FROM report_rate_limits WHERE user_id = p_user_id;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT TRUE, v_max_reports_per_day, NULL::TEXT;
        RETURN;
    END IF;
    
    IF v_limit.is_blocked THEN
        IF v_limit.blocked_until_ts IS NOT NULL AND v_limit.blocked_until_ts < EXTRACT(EPOCH FROM NOW()) * 1000 THEN
            UPDATE report_rate_limits SET is_blocked = FALSE, blocked_until_ts = NULL, block_reason = NULL WHERE user_id = p_user_id;
        ELSE
            RETURN QUERY SELECT FALSE, 0, v_limit.block_reason;
            RETURN;
        END IF;
    END IF;
    
    IF v_limit.last_report_ts IS NOT NULL AND v_limit.last_report_ts > EXTRACT(EPOCH FROM NOW() - INTERVAL '1 day') * 1000 THEN
        IF v_limit.report_count >= v_max_reports_per_day THEN
            RETURN QUERY SELECT FALSE, 0, 'Daily report limit exceeded'::TEXT;
            RETURN;
        END IF;
        RETURN QUERY SELECT TRUE, v_max_reports_per_day - v_limit.report_count, NULL::TEXT;
        RETURN;
    END IF;
    
    RETURN QUERY SELECT TRUE, v_max_reports_per_day, NULL::TEXT;
END;
$$ LANGUAGE plpgsql;

-- 函数：记录举报
CREATE OR REPLACE FUNCTION record_report(p_user_id VARCHAR)
RETURNS VOID AS $$
BEGIN
    INSERT INTO report_rate_limits (user_id, report_count, last_report_ts, created_ts, updated_ts)
    VALUES (p_user_id, 1, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000, EXTRACT(EPOCH FROM NOW()) * 1000)
    ON CONFLICT (user_id) DO UPDATE SET
        report_count = CASE 
            WHEN report_rate_limits.last_report_ts < EXTRACT(EPOCH FROM NOW() - INTERVAL '1 day') * 1000 
            THEN 1 
            ELSE report_rate_limits.report_count + 1 
        END,
        last_report_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
END;
$$ LANGUAGE plpgsql;

-- 函数：获取报告统计
CREATE OR REPLACE FUNCTION get_report_stats(p_days INTEGER DEFAULT 30)
RETURNS TABLE (
    stat_date DATE,
    total_reports BIGINT,
    open_reports BIGINT,
    resolved_reports BIGINT,
    dismissed_reports BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        ers.stat_date,
        ers.total_reports::BIGINT,
        ers.open_reports::BIGINT,
        ers.resolved_reports::BIGINT,
        ers.dismissed_reports::BIGINT
    FROM event_report_stats ers
    WHERE ers.stat_date >= CURRENT_DATE - p_days
    ORDER BY ers.stat_date DESC;
END;
$$ LANGUAGE plpgsql;

-- 视图：开放报告
CREATE OR REPLACE VIEW v_open_reports AS
SELECT 
    er.id,
    er.event_id,
    er.room_id,
    er.reporter_user_id,
    er.reported_user_id,
    er.reason,
    er.description,
    er.score,
    er.received_ts,
    er.status
FROM event_reports er
WHERE er.status = 'open'
ORDER BY er.score DESC, er.received_ts DESC;

-- 视图：报告摘要
CREATE OR REPLACE VIEW v_report_summary AS
SELECT 
    er.status,
    er.reason,
    COUNT(*) as count,
    MIN(er.received_ts) as first_report,
    MAX(er.received_ts) as last_report
FROM event_reports er
GROUP BY er.status, er.reason;

-- 枚举类型：报告状态
-- CREATE TYPE report_status AS ENUM ('open', 'investigating', 'resolved', 'dismissed');
-- 注：PostgreSQL 枚举类型在迁移中可能有问题，使用 VARCHAR 代替
