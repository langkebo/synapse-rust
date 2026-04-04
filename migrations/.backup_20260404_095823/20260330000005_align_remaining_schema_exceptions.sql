DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS room_summary_state (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        state_key TEXT NOT NULL,
        event_id TEXT,
        content JSONB NOT NULL DEFAULT '{}',
        updated_ts BIGINT NOT NULL,
        CONSTRAINT uq_room_summary_state_room_type_state UNIQUE (room_id, event_type, state_key),
        CONSTRAINT fk_room_summary_state_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS room_summary_stats (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL UNIQUE,
        total_events BIGINT NOT NULL DEFAULT 0,
        total_state_events BIGINT NOT NULL DEFAULT 0,
        total_messages BIGINT NOT NULL DEFAULT 0,
        total_media BIGINT NOT NULL DEFAULT 0,
        storage_size BIGINT NOT NULL DEFAULT 0,
        last_updated_ts BIGINT NOT NULL,
        CONSTRAINT fk_room_summary_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS room_summary_update_queue (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        state_key TEXT,
        priority INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        created_ts BIGINT NOT NULL,
        processed_ts BIGINT,
        error_message TEXT,
        retry_count INTEGER NOT NULL DEFAULT 0,
        CONSTRAINT fk_room_summary_update_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS room_children (
        id BIGSERIAL PRIMARY KEY,
        parent_room_id TEXT NOT NULL,
        child_room_id TEXT NOT NULL,
        state_key TEXT,
        content JSONB NOT NULL DEFAULT '{}',
        suggested BOOLEAN NOT NULL DEFAULT FALSE,
        created_ts BIGINT NOT NULL DEFAULT 0,
        updated_ts BIGINT,
        CONSTRAINT uq_room_children_parent_child UNIQUE (parent_room_id, child_room_id),
        CONSTRAINT fk_room_children_parent FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
        CONSTRAINT fk_room_children_child FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS retention_cleanup_queue (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_id TEXT,
        event_type TEXT,
        origin_server_ts BIGINT NOT NULL,
        scheduled_ts BIGINT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        created_ts BIGINT NOT NULL,
        processed_ts BIGINT,
        error_message TEXT,
        retry_count INTEGER NOT NULL DEFAULT 0,
        CONSTRAINT uq_retention_cleanup_queue_room_event UNIQUE (room_id, event_id),
        CONSTRAINT fk_retention_cleanup_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS retention_cleanup_logs (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        events_deleted BIGINT NOT NULL DEFAULT 0,
        state_events_deleted BIGINT NOT NULL DEFAULT 0,
        media_deleted BIGINT NOT NULL DEFAULT 0,
        bytes_freed BIGINT NOT NULL DEFAULT 0,
        started_ts BIGINT NOT NULL,
        completed_ts BIGINT,
        status TEXT NOT NULL,
        error_message TEXT,
        CONSTRAINT fk_retention_cleanup_logs_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS retention_stats (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL UNIQUE,
        total_events BIGINT NOT NULL DEFAULT 0,
        events_in_retention BIGINT NOT NULL DEFAULT 0,
        events_expired BIGINT NOT NULL DEFAULT 0,
        last_cleanup_ts BIGINT,
        next_cleanup_ts BIGINT,
        CONSTRAINT fk_retention_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS deleted_events_index (
        id BIGSERIAL PRIMARY KEY,
        room_id TEXT NOT NULL,
        event_id TEXT NOT NULL,
        deletion_ts BIGINT NOT NULL,
        reason TEXT NOT NULL,
        CONSTRAINT uq_deleted_events_index_room_event UNIQUE (room_id, event_id),
        CONSTRAINT fk_deleted_events_index_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS device_trust_status (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        trust_level TEXT NOT NULL DEFAULT 'unverified',
        verified_by_device_id TEXT,
        verified_at TIMESTAMPTZ,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        CONSTRAINT uq_device_trust_status_user_device UNIQUE (user_id, device_id)
    );

    CREATE TABLE IF NOT EXISTS cross_signing_trust (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        target_user_id TEXT NOT NULL,
        master_key_id TEXT,
        is_trusted BOOLEAN NOT NULL DEFAULT FALSE,
        trusted_at TIMESTAMPTZ,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        CONSTRAINT uq_cross_signing_trust_user_target UNIQUE (user_id, target_user_id)
    );

    CREATE TABLE IF NOT EXISTS verification_requests (
        transaction_id TEXT PRIMARY KEY,
        from_user TEXT NOT NULL,
        from_device TEXT NOT NULL,
        to_user TEXT NOT NULL,
        to_device TEXT,
        method TEXT NOT NULL,
        state TEXT NOT NULL,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT
    );

    CREATE TABLE IF NOT EXISTS verification_sas (
        tx_id TEXT PRIMARY KEY,
        from_device TEXT NOT NULL,
        to_device TEXT,
        method TEXT NOT NULL,
        state TEXT NOT NULL,
        exchange_hashes JSONB NOT NULL DEFAULT '[]',
        commitment TEXT,
        pubkey TEXT,
        sas_bytes BYTEA,
        mac TEXT
    );

    CREATE TABLE IF NOT EXISTS verification_qr (
        tx_id TEXT PRIMARY KEY,
        from_device TEXT NOT NULL,
        to_device TEXT,
        state TEXT NOT NULL,
        qr_code_data TEXT,
        scanned_data TEXT
    );

    CREATE TABLE IF NOT EXISTS moderation_actions (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        action_type TEXT NOT NULL,
        reason TEXT,
        report_id BIGINT,
        created_ts BIGINT NOT NULL,
        expires_at BIGINT,
        revoked BOOLEAN NOT NULL DEFAULT FALSE,
        revoked_reason TEXT,
        revoked_at BIGINT,
        CONSTRAINT fk_moderation_actions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS moderation_rules (
        id BIGSERIAL PRIMARY KEY,
        rule_id TEXT NOT NULL UNIQUE,
        server_id TEXT,
        rule_type TEXT NOT NULL,
        pattern TEXT NOT NULL,
        action TEXT NOT NULL,
        reason TEXT,
        created_by TEXT NOT NULL,
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT NOT NULL,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        priority INTEGER NOT NULL DEFAULT 100
    );

    CREATE TABLE IF NOT EXISTS moderation_logs (
        id BIGSERIAL PRIMARY KEY,
        rule_id TEXT NOT NULL,
        event_id TEXT NOT NULL,
        room_id TEXT NOT NULL,
        sender TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        action_taken TEXT NOT NULL,
        confidence REAL NOT NULL,
        created_ts BIGINT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS replication_positions (
        id BIGSERIAL PRIMARY KEY,
        worker_id TEXT NOT NULL,
        stream_name TEXT NOT NULL,
        stream_position BIGINT NOT NULL DEFAULT 0,
        updated_ts BIGINT NOT NULL,
        CONSTRAINT uq_replication_positions_worker_stream UNIQUE (worker_id, stream_name),
        CONSTRAINT fk_replication_positions_worker FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS worker_load_stats (
        id BIGSERIAL PRIMARY KEY,
        worker_id TEXT NOT NULL,
        cpu_usage REAL,
        memory_usage BIGINT,
        active_connections INTEGER,
        requests_per_second REAL,
        average_latency_ms REAL,
        queue_depth INTEGER,
        recorded_ts BIGINT NOT NULL,
        CONSTRAINT fk_worker_load_stats_worker FOREIGN KEY (worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS worker_task_assignments (
        id BIGSERIAL PRIMARY KEY,
        task_id TEXT NOT NULL UNIQUE,
        task_type TEXT NOT NULL,
        task_data JSONB NOT NULL DEFAULT '{}',
        priority INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        assigned_worker_id TEXT,
        assigned_ts BIGINT,
        created_ts BIGINT NOT NULL,
        completed_ts BIGINT,
        result JSONB,
        error_message TEXT,
        CONSTRAINT fk_worker_task_assignments_worker FOREIGN KEY (assigned_worker_id) REFERENCES workers(worker_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS worker_connections (
        id BIGSERIAL PRIMARY KEY,
        source_worker_id TEXT NOT NULL,
        target_worker_id TEXT NOT NULL,
        connection_type TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'connected',
        established_ts BIGINT NOT NULL,
        last_activity_ts BIGINT,
        bytes_sent BIGINT NOT NULL DEFAULT 0,
        bytes_received BIGINT NOT NULL DEFAULT 0,
        messages_sent BIGINT NOT NULL DEFAULT 0,
        messages_received BIGINT NOT NULL DEFAULT 0,
        CONSTRAINT uq_worker_connections_pair UNIQUE (source_worker_id, target_worker_id, connection_type),
        CONSTRAINT fk_worker_connections_source FOREIGN KEY (source_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE,
        CONSTRAINT fk_worker_connections_target FOREIGN KEY (target_worker_id) REFERENCES workers(worker_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS widgets (
        id BIGSERIAL PRIMARY KEY,
        widget_id TEXT NOT NULL UNIQUE,
        room_id TEXT,
        user_id TEXT NOT NULL,
        widget_type TEXT NOT NULL,
        url TEXT NOT NULL,
        name TEXT NOT NULL,
        data JSONB NOT NULL DEFAULT '{}',
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        CONSTRAINT fk_widgets_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
        CONSTRAINT fk_widgets_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS widget_permissions (
        id BIGSERIAL PRIMARY KEY,
        widget_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        permissions JSONB NOT NULL DEFAULT '[]',
        created_ts BIGINT NOT NULL,
        updated_ts BIGINT,
        CONSTRAINT uq_widget_permissions_widget_user UNIQUE (widget_id, user_id),
        CONSTRAINT fk_widget_permissions_widget FOREIGN KEY (widget_id) REFERENCES widgets(widget_id) ON DELETE CASCADE,
        CONSTRAINT fk_widget_permissions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS widget_sessions (
        id BIGSERIAL PRIMARY KEY,
        session_id TEXT NOT NULL UNIQUE,
        widget_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        device_id TEXT,
        created_ts BIGINT NOT NULL,
        last_active_ts BIGINT,
        expires_at BIGINT,
        is_active BOOLEAN NOT NULL DEFAULT TRUE,
        CONSTRAINT fk_widget_sessions_widget FOREIGN KEY (widget_id) REFERENCES widgets(widget_id) ON DELETE CASCADE,
        CONSTRAINT fk_widget_sessions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS server_notifications (
        id BIGSERIAL PRIMARY KEY,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        notification_type TEXT NOT NULL DEFAULT 'info',
        priority INTEGER NOT NULL DEFAULT 0,
        target_audience TEXT NOT NULL DEFAULT 'all',
        target_user_ids JSONB NOT NULL DEFAULT '[]',
        starts_at BIGINT,
        expires_at BIGINT,
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        is_dismissable BOOLEAN NOT NULL DEFAULT TRUE,
        action_url TEXT,
        action_text TEXT,
        created_by TEXT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );

    CREATE TABLE IF NOT EXISTS user_notification_status (
        id BIGSERIAL PRIMARY KEY,
        user_id TEXT NOT NULL,
        notification_id BIGINT NOT NULL,
        is_read BOOLEAN NOT NULL DEFAULT FALSE,
        is_dismissed BOOLEAN NOT NULL DEFAULT FALSE,
        read_ts BIGINT,
        dismissed_ts BIGINT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT uq_user_notification_status_user_notification UNIQUE (user_id, notification_id),
        CONSTRAINT fk_user_notification_status_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
        CONSTRAINT fk_user_notification_status_notification FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS notification_templates (
        id BIGSERIAL PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        title_template TEXT NOT NULL,
        content_template TEXT NOT NULL,
        notification_type TEXT NOT NULL DEFAULT 'info',
        variables JSONB NOT NULL DEFAULT '[]',
        is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
    );

    CREATE TABLE IF NOT EXISTS notification_delivery_log (
        id BIGSERIAL PRIMARY KEY,
        notification_id BIGINT NOT NULL,
        user_id TEXT,
        delivery_method TEXT NOT NULL,
        status TEXT NOT NULL,
        error_message TEXT,
        delivered_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_notification_delivery_log_notification FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE,
        CONSTRAINT fk_notification_delivery_log_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS scheduled_notifications (
        id BIGSERIAL PRIMARY KEY,
        notification_id BIGINT NOT NULL,
        scheduled_for BIGINT NOT NULL,
        is_sent BOOLEAN NOT NULL DEFAULT FALSE,
        sent_ts BIGINT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_scheduled_notifications_notification FOREIGN KEY (notification_id) REFERENCES server_notifications(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS secure_key_backups (
        user_id TEXT NOT NULL,
        backup_id TEXT NOT NULL,
        version TEXT NOT NULL,
        algorithm TEXT NOT NULL,
        auth_data TEXT NOT NULL,
        key_count BIGINT NOT NULL DEFAULT 0,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        updated_ts BIGINT DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT pk_secure_key_backups PRIMARY KEY (user_id, backup_id),
        CONSTRAINT fk_secure_key_backups_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS secure_backup_session_keys (
        user_id TEXT NOT NULL,
        backup_id TEXT NOT NULL,
        room_id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        encrypted_key TEXT NOT NULL,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT pk_secure_backup_session_keys PRIMARY KEY (user_id, backup_id, room_id, session_id),
        CONSTRAINT fk_secure_backup_session_keys_backup FOREIGN KEY (user_id, backup_id) REFERENCES secure_key_backups(user_id, backup_id) ON DELETE CASCADE,
        CONSTRAINT fk_secure_backup_session_keys_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS application_service_users (
        as_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        displayname TEXT,
        avatar_url TEXT,
        created_ts BIGINT NOT NULL,
        CONSTRAINT pk_application_service_users PRIMARY KEY (as_id, user_id),
        CONSTRAINT fk_application_service_users_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS application_service_statistics (
        id BIGSERIAL PRIMARY KEY,
        as_id TEXT NOT NULL UNIQUE,
        name TEXT,
        is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
        rate_limited BOOLEAN NOT NULL DEFAULT TRUE,
        virtual_user_count BIGINT NOT NULL DEFAULT 0,
        pending_event_count BIGINT NOT NULL DEFAULT 0,
        pending_transaction_count BIGINT NOT NULL DEFAULT 0,
        last_seen_ts BIGINT,
        created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT,
        CONSTRAINT fk_application_service_statistics_as FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
    );
END $$;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_summary_state_room
ON room_summary_state(room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_summary_update_queue_status_priority_created
ON room_summary_update_queue(status, priority DESC, created_ts ASC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_children_parent_suggested
ON room_children(parent_room_id, suggested, child_room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_room_children_child
ON room_children(child_room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_retention_cleanup_queue_status_origin
ON retention_cleanup_queue(status, origin_server_ts ASC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_retention_cleanup_logs_room_started
ON retention_cleanup_logs(room_id, started_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_deleted_events_index_room_ts
ON deleted_events_index(room_id, deletion_ts ASC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_device_trust_status_user_level
ON device_trust_status(user_id, trust_level);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cross_signing_trust_user_trusted
ON cross_signing_trust(user_id, is_trusted);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_verification_requests_to_user_state
ON verification_requests(to_user, state, updated_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_moderation_actions_user_created
ON moderation_actions(user_id, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_moderation_rules_active_priority
ON moderation_rules(is_active, priority DESC, created_ts ASC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_moderation_rules_type_active
ON moderation_rules(rule_type, is_active);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_moderation_logs_event_created
ON moderation_logs(event_id, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_moderation_logs_room_created
ON moderation_logs(room_id, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_moderation_logs_sender_created
ON moderation_logs(sender, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_worker_load_stats_worker_recorded
ON worker_load_stats(worker_id, recorded_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_worker_task_assignments_status_priority_created
ON worker_task_assignments(status, priority DESC, created_ts ASC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_worker_task_assignments_worker_status
ON worker_task_assignments(assigned_worker_id, status);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_widgets_room_active_created
ON widgets(room_id, created_ts DESC)
WHERE is_active = TRUE;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_widgets_user_active_created
ON widgets(user_id, created_ts DESC)
WHERE is_active = TRUE;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_widget_permissions_widget
ON widget_permissions(widget_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_widget_permissions_user
ON widget_permissions(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_widget_sessions_widget_active_last_active
ON widget_sessions(widget_id, last_active_ts DESC)
WHERE is_active = TRUE;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_server_notifications_enabled_priority_created
ON server_notifications(priority DESC, created_ts DESC)
WHERE is_enabled = TRUE;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_notification_status_user_created
ON user_notification_status(user_id, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notification_templates_enabled
ON notification_templates(is_enabled)
WHERE is_enabled = TRUE;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notification_delivery_log_notification_delivered
ON notification_delivery_log(notification_id, delivered_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_scheduled_notifications_pending
ON scheduled_notifications(scheduled_for)
WHERE is_sent = FALSE;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_secure_key_backups_user_created
ON secure_key_backups(user_id, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_secure_backup_session_keys_backup
ON secure_backup_session_keys(user_id, backup_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_application_service_users_as
ON application_service_users(as_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_worker_connections_source
ON worker_connections(source_worker_id, status);

CREATE OR REPLACE VIEW active_workers AS
SELECT id, worker_id, worker_name, worker_type, host, port, status,
       last_heartbeat_ts, started_ts, stopped_ts, config, metadata, version, is_enabled
FROM workers
WHERE status = 'running' OR status = 'starting';

CREATE OR REPLACE VIEW worker_type_statistics AS
SELECT
    w.worker_type,
    COUNT(*)::BIGINT AS total_count,
    COUNT(*) FILTER (WHERE w.status = 'running')::BIGINT AS running_count,
    COUNT(*) FILTER (WHERE w.status = 'starting')::BIGINT AS starting_count,
    COUNT(*) FILTER (WHERE w.status = 'stopping')::BIGINT AS stopping_count,
    COUNT(*) FILTER (WHERE w.status = 'stopped')::BIGINT AS stopped_count,
    AVG(ls.cpu_usage)::DOUBLE PRECISION AS avg_cpu_usage,
    AVG(ls.memory_usage)::DOUBLE PRECISION AS avg_memory_usage,
    COALESCE(SUM(conn.connection_count), 0)::BIGINT AS total_connections
FROM workers w
LEFT JOIN LATERAL (
    SELECT cpu_usage, memory_usage
    FROM worker_load_stats
    WHERE worker_id = w.worker_id
    ORDER BY recorded_ts DESC
    LIMIT 1
) ls ON TRUE
LEFT JOIN LATERAL (
    SELECT COUNT(*)::BIGINT AS connection_count
    FROM worker_connections
    WHERE source_worker_id = w.worker_id AND status = 'connected'
) conn ON TRUE
GROUP BY w.worker_type;
