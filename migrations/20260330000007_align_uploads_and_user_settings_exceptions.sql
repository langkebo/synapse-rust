DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS upload_progress (
        upload_id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        filename TEXT,
        content_type TEXT,
        total_size BIGINT,
        uploaded_size BIGINT NOT NULL DEFAULT 0,
        total_chunks INTEGER NOT NULL,
        uploaded_chunks INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        expires_at BIGINT NOT NULL,
        CONSTRAINT fk_upload_progress_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS upload_chunks (
        upload_id TEXT NOT NULL,
        chunk_index INTEGER NOT NULL,
        chunk_data BYTEA NOT NULL,
        chunk_size BIGINT NOT NULL,
        created_ts BIGINT NOT NULL,
        CONSTRAINT pk_upload_chunks PRIMARY KEY (upload_id, chunk_index),
        CONSTRAINT fk_upload_chunks_upload FOREIGN KEY (upload_id) REFERENCES upload_progress(upload_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS user_settings (
        user_id TEXT PRIMARY KEY,
        theme TEXT,
        language TEXT,
        time_zone TEXT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_user_settings_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );
END $$;

CREATE INDEX IF NOT EXISTS idx_upload_progress_expires
ON upload_progress(expires_at ASC);

CREATE INDEX IF NOT EXISTS idx_upload_progress_user_created_active
ON upload_progress(user_id, created_ts DESC)
WHERE status <> 'finalized';

CREATE INDEX IF NOT EXISTS idx_upload_chunks_upload_order
ON upload_chunks(upload_id, chunk_index ASC);
