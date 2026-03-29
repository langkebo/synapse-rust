DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS push_device (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        push_token TEXT NOT NULL,
        push_type TEXT NOT NULL,
        app_id TEXT,
        platform TEXT,
        platform_version TEXT,
        app_version TEXT,
        locale TEXT,
        timezone TEXT,
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        last_used_at TIMESTAMPTZ,
        last_error TEXT,
        error_count INTEGER NOT NULL DEFAULT 0,
        metadata JSONB NOT NULL DEFAULT '{}',
        CONSTRAINT uq_push_device_user_device UNIQUE (user_id, device_id),
        CONSTRAINT fk_push_device_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS rate_limits (
        user_id TEXT PRIMARY KEY,
        messages_per_second DOUBLE PRECISION,
        burst_count INTEGER,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_rate_limits_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS user_notification_settings (
        user_id TEXT PRIMARY KEY,
        enabled BOOLEAN NOT NULL DEFAULT TRUE,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_user_notification_settings_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS server_notices (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT,
        event_id TEXT,
        content TEXT,
        sent_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_server_notices_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS qr_login_transactions (
        transaction_id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        device_id TEXT,
        status TEXT NOT NULL,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        expires_at BIGINT NOT NULL,
        CONSTRAINT fk_qr_login_transactions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS reaction_aggregations (
        event_id TEXT PRIMARY KEY,
        relates_to_event_id TEXT NOT NULL,
        sender TEXT NOT NULL,
        room_id TEXT NOT NULL,
        reaction_key TEXT NOT NULL,
        count BIGINT NOT NULL DEFAULT 1,
        origin_server_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_reaction_aggregations_sender FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE,
        CONSTRAINT fk_reaction_aggregations_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS registration_token_batches (
        id BIGSERIAL PRIMARY KEY,
        batch_id TEXT NOT NULL UNIQUE,
        description TEXT,
        token_count INTEGER NOT NULL,
        tokens_used INTEGER NOT NULL DEFAULT 0,
        created_by TEXT,
        created_ts BIGINT NOT NULL,
        expires_at BIGINT,
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        allowed_email_domains TEXT[],
        auto_join_rooms TEXT[]
    );
END $$;

CREATE INDEX IF NOT EXISTS idx_push_device_user_enabled
ON push_device(user_id)
WHERE is_enabled = TRUE;

CREATE INDEX IF NOT EXISTS idx_rate_limits_updated
ON rate_limits(updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_server_notices_sent
ON server_notices(sent_ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_notification_settings_updated
ON user_notification_settings(updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_expires
ON qr_login_transactions(expires_at ASC);

CREATE INDEX IF NOT EXISTS idx_qr_login_transactions_user_created
ON qr_login_transactions(user_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_reaction_aggregations_room_relates_origin
ON reaction_aggregations(room_id, relates_to_event_id, origin_server_ts DESC);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_created
ON registration_token_batches(created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_enabled_created
ON registration_token_batches(created_ts DESC)
WHERE is_enabled = TRUE;
