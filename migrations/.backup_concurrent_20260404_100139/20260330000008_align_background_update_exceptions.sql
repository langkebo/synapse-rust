DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS background_update_locks (
        lock_name TEXT PRIMARY KEY,
        owner TEXT,
        acquired_ts BIGINT NOT NULL,
        expires_at BIGINT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS background_update_history (
        id BIGSERIAL PRIMARY KEY,
        job_name TEXT NOT NULL,
        execution_start_ts BIGINT NOT NULL,
        execution_end_ts BIGINT,
        status TEXT NOT NULL,
        items_processed INTEGER NOT NULL DEFAULT 0,
        error_message TEXT,
        metadata JSONB
    );

    CREATE TABLE IF NOT EXISTS background_update_stats (
        id BIGSERIAL PRIMARY KEY,
        job_name TEXT NOT NULL,
        total_updates INTEGER NOT NULL DEFAULT 0,
        completed_updates INTEGER NOT NULL DEFAULT 0,
        failed_updates INTEGER NOT NULL DEFAULT 0,
        last_run_ts BIGINT,
        next_run_ts BIGINT,
        average_duration_ms BIGINT NOT NULL DEFAULT 0,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );
END $$;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_background_update_locks_expires
ON background_update_locks(expires_at);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_background_update_history_job_start
ON background_update_history(job_name, execution_start_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_background_update_stats_created
ON background_update_stats(created_ts DESC);
