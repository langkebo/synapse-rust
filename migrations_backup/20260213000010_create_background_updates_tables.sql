-- 背景更新表
-- 支持后台数据库迁移和清理任务

-- 背景更新任务表
CREATE TABLE IF NOT EXISTS background_updates (
    job_name VARCHAR(255) PRIMARY KEY,
    job_type VARCHAR(50) NOT NULL,
    description TEXT,
    table_name VARCHAR(255),
    column_name VARCHAR(255),
    status VARCHAR(50) DEFAULT 'pending',
    progress INTEGER DEFAULT 0,
    total_items INTEGER DEFAULT 0,
    processed_items INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    started_ts BIGINT,
    completed_ts BIGINT,
    last_updated_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    batch_size INTEGER DEFAULT 100,
    sleep_ms INTEGER DEFAULT 1000,
    depends_on TEXT[],
    metadata JSONB
);

CREATE INDEX IF NOT EXISTS idx_background_updates_status ON background_updates(status);
CREATE INDEX IF NOT EXISTS idx_background_updates_job_type ON background_updates(job_type);
CREATE INDEX IF NOT EXISTS idx_background_updates_created_ts ON background_updates(created_ts);

-- 背景更新执行历史表
CREATE TABLE IF NOT EXISTS background_update_history (
    id BIGSERIAL PRIMARY KEY,
    job_name VARCHAR(255) NOT NULL,
    execution_start_ts BIGINT NOT NULL,
    execution_end_ts BIGINT,
    status VARCHAR(50) NOT NULL,
    items_processed INTEGER DEFAULT 0,
    error_message TEXT,
    metadata JSONB,
    FOREIGN KEY (job_name) REFERENCES background_updates(job_name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_background_update_history_job_name ON background_update_history(job_name);
CREATE INDEX IF NOT EXISTS idx_background_update_history_status ON background_update_history(status);
CREATE INDEX IF NOT EXISTS idx_background_update_history_start_ts ON background_update_history(execution_start_ts DESC);

-- 背景更新锁表（防止并发执行）
CREATE TABLE IF NOT EXISTS background_update_locks (
    job_name VARCHAR(255) PRIMARY KEY,
    locked_by VARCHAR(255),
    locked_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    FOREIGN KEY (job_name) REFERENCES background_updates(job_name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_background_update_locks_expires ON background_update_locks(expires_ts);

-- 背景更新统计表
CREATE TABLE IF NOT EXISTS background_update_stats (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL UNIQUE,
    total_jobs INTEGER DEFAULT 0,
    completed_jobs INTEGER DEFAULT 0,
    failed_jobs INTEGER DEFAULT 0,
    total_items_processed BIGINT DEFAULT 0,
    total_execution_time_ms BIGINT DEFAULT 0,
    avg_items_per_second FLOAT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_background_update_stats_date ON background_update_stats(stat_date DESC);

-- 触发器：自动更新 updated_ts
CREATE OR REPLACE FUNCTION update_background_update_stats_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_background_update_stats_timestamp
    BEFORE UPDATE ON background_update_stats
    FOR EACH ROW
    EXECUTE FUNCTION update_background_update_stats_timestamp();

-- 函数：获取下一个待执行的更新任务
CREATE OR REPLACE FUNCTION get_next_background_update()
RETURNS TABLE (
    job_name VARCHAR(255),
    job_type VARCHAR(50),
    table_name VARCHAR(255),
    column_name VARCHAR(255),
    batch_size INTEGER,
    sleep_ms INTEGER,
    metadata JSONB
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        bu.job_name,
        bu.job_type,
        bu.table_name,
        bu.column_name,
        bu.batch_size,
        bu.sleep_ms,
        bu.metadata
    FROM background_updates bu
    WHERE bu.status = 'pending'
    AND (bu.depends_on IS NULL OR NOT EXISTS (
        SELECT 1 FROM background_updates dep 
        WHERE dep.job_name = ANY(bu.depends_on) 
        AND dep.status != 'completed'
    ))
    AND NOT EXISTS (
        SELECT 1 FROM background_update_locks bul 
        WHERE bul.job_name = bu.job_name 
        AND bul.expires_ts > EXTRACT(EPOCH FROM NOW()) * 1000
    )
    ORDER BY bu.created_ts ASC
    LIMIT 1;
END;
$$ LANGUAGE plpgsql;

-- 函数：锁定更新任务
CREATE OR REPLACE FUNCTION lock_background_update(
    p_job_name VARCHAR,
    p_locked_by VARCHAR,
    p_lock_duration_ms BIGINT
) RETURNS BOOLEAN AS $$
DECLARE
    v_now BIGINT;
    v_expires BIGINT;
BEGIN
    v_now := EXTRACT(EPOCH FROM NOW()) * 1000;
    v_expires := v_now + p_lock_duration_ms;
    
    INSERT INTO background_update_locks (job_name, locked_by, locked_ts, expires_ts)
    VALUES (p_job_name, p_locked_by, v_now, v_expires)
    ON CONFLICT (job_name) DO UPDATE SET
        locked_by = p_locked_by,
        locked_ts = v_now,
        expires_ts = v_expires
    WHERE background_update_locks.expires_ts < v_now;
    
    RETURN FOUND OR (SELECT expires_ts > v_now FROM background_update_locks WHERE job_name = p_job_name);
END;
$$ LANGUAGE plpgsql;

-- 函数：释放更新任务锁
CREATE OR REPLACE FUNCTION unlock_background_update(p_job_name VARCHAR)
RETURNS VOID AS $$
BEGIN
    DELETE FROM background_update_locks WHERE job_name = p_job_name;
END;
$$ LANGUAGE plpgsql;

-- 函数：更新任务进度
CREATE OR REPLACE FUNCTION update_background_update_progress(
    p_job_name VARCHAR,
    p_items_processed INTEGER,
    p_total_items INTEGER
) RETURNS VOID AS $$
DECLARE
    v_now BIGINT;
BEGIN
    v_now := EXTRACT(EPOCH FROM NOW()) * 1000;
    
    UPDATE background_updates SET
        processed_items = processed_items + p_items_processed,
        total_items = COALESCE(p_total_items, total_items),
        progress = CASE 
            WHEN COALESCE(p_total_items, total_items) > 0 
            THEN ROUND((processed_items + p_items_processed)::FLOAT / COALESCE(p_total_items, total_items) * 100)
            ELSE progress 
        END,
        last_updated_ts = v_now,
        status = CASE 
            WHEN processed_items + p_items_processed >= total_items AND total_items > 0 
            THEN 'completed' 
            ELSE status 
        END,
        completed_ts = CASE 
            WHEN processed_items + p_items_processed >= total_items AND total_items > 0 
            THEN v_now 
            ELSE completed_ts 
        END
    WHERE job_name = p_job_name;
END;
$$ LANGUAGE plpgsql;

-- 函数：清理过期锁
CREATE OR REPLACE FUNCTION cleanup_expired_locks()
RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    DELETE FROM background_update_locks 
    WHERE expires_ts < EXTRACT(EPOCH FROM NOW()) * 1000;
    
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- 函数：重试失败任务
CREATE OR REPLACE FUNCTION retry_failed_background_updates()
RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    UPDATE background_updates SET
        status = 'pending',
        error_message = NULL,
        retry_count = retry_count + 1
    WHERE status = 'failed'
    AND retry_count < max_retries;
    
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- 视图：活跃更新任务
CREATE OR REPLACE VIEW v_active_background_updates AS
SELECT 
    bu.job_name,
    bu.job_type,
    bu.description,
    bu.table_name,
    bu.status,
    bu.progress,
    bu.total_items,
    bu.processed_items,
    bu.created_ts,
    bu.started_ts,
    bu.last_updated_ts,
    bu.error_message,
    bu.retry_count
FROM background_updates bu
WHERE bu.status IN ('pending', 'running')
ORDER BY bu.created_ts ASC;

-- 视图：更新任务摘要
CREATE OR REPLACE VIEW v_background_update_summary AS
SELECT 
    bu.status,
    bu.job_type,
    COUNT(*) as count,
    AVG(bu.progress) as avg_progress,
    SUM(bu.processed_items) as total_processed
FROM background_updates bu
GROUP BY bu.status, bu.job_type;
