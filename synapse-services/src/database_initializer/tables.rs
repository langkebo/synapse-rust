#[cfg(feature = "runtime-ddl")]
use super::models::DatabaseInitService;
#[cfg(feature = "runtime-ddl")]
use synapse_common::current_timestamp_millis;
#[cfg(feature = "runtime-ddl")]
use tracing::warn;

#[cfg(feature = "runtime-ddl")]
impl DatabaseInitService {
    pub(crate) async fn step_create_e2ee_tables(&self) -> Result<String, sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_keys (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                key_id TEXT NOT NULL,
                public_key TEXT NOT NULL,
                key_data TEXT,
                signatures JSONB,
                added_ts BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                ts_updated_ms BIGINT,
                is_verified BOOLEAN DEFAULT FALSE,
                is_blocked BOOLEAN DEFAULT FALSE,
                is_fallback BOOLEAN DEFAULT FALSE,
                display_name TEXT,
                CONSTRAINT uq_device_keys_user_device_key UNIQUE (user_id, device_id, key_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok("E2EE设备密钥表创建完成".to_string())
    }

    /// 创建 E2EE 核心表 - 包括 Olm 和 Megolm 会话表
    /// 这些表在迁移文件中定义，确保在迁移失败时也能创建
    pub(crate) async fn step_create_e2ee_core_tables(&self) -> Result<String, sqlx::Error> {
        // Create olm_accounts table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS olm_accounts (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                identity_key TEXT NOT NULL,
                serialized_account TEXT NOT NULL,
                is_one_time_keys_published BOOLEAN DEFAULT FALSE,
                is_fallback_key_published BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_olm_accounts_user_device UNIQUE (user_id, device_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_olm_accounts_user ON olm_accounts(user_id)")
            .execute(&*self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_olm_accounts_device ON olm_accounts(device_id)")
            .execute(&*self.pool)
            .await?;

        // Create olm_sessions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS olm_sessions (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                sender_key TEXT NOT NULL,
                receiver_key TEXT NOT NULL,
                serialized_state TEXT NOT NULL,
                message_index INTEGER DEFAULT 0,
                created_ts BIGINT NOT NULL,
                last_used_ts BIGINT NOT NULL,
                is_fallback BOOLEAN DEFAULT FALSE,
                CONSTRAINT uq_olm_sessions UNIQUE (session_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_olm_sessions_user ON olm_sessions(user_id)")
            .execute(&*self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_olm_sessions_device ON olm_sessions(device_id)")
            .execute(&*self.pool)
            .await?;

        // Create megolm_sessions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS megolm_sessions (
                id BIGSERIAL PRIMARY KEY,
                room_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                sender_key TEXT NOT NULL,
                sender_claimed_key TEXT NOT NULL,
                forwarding_chains JSONB DEFAULT '[]',
                is_fallback BOOLEAN DEFAULT FALSE,
                session_data JSONB NOT NULL,
                message_index INTEGER DEFAULT 0,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                used_ts BIGINT,
                CONSTRAINT uq_megolm_sessions UNIQUE (room_id, session_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id)")
            .execute(&*self.pool)
            .await?;

        // Create cross_signing_keys table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cross_signing_keys (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                key_type TEXT NOT NULL,
                key_data TEXT NOT NULL,
                signatures JSONB,
                added_ts BIGINT NOT NULL,
                CONSTRAINT uq_cross_signing_keys_user_type UNIQUE (user_id, key_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("ALTER TABLE cross_signing_keys ADD COLUMN IF NOT EXISTS key_data TEXT")
            .execute(&*self.pool)
            .await?;
        sqlx::query("ALTER TABLE cross_signing_keys ADD COLUMN IF NOT EXISTS signatures JSONB")
            .execute(&*self.pool)
            .await?;
        sqlx::query("ALTER TABLE cross_signing_keys ADD COLUMN IF NOT EXISTS added_ts BIGINT")
            .execute(&*self.pool)
            .await?;
        sqlx::query(
            r#"
            DO $$
            DECLARE
                added_ts_sources TEXT := 'added_ts';
            BEGIN
                IF EXISTS (
                    SELECT 1
                    FROM information_schema.columns
                    WHERE table_schema = current_schema()
                      AND table_name = 'cross_signing_keys'
                      AND column_name = 'updated_ts'
                ) THEN
                    added_ts_sources := added_ts_sources || ', updated_ts';
                END IF;

                IF EXISTS (
                    SELECT 1
                    FROM information_schema.columns
                    WHERE table_schema = current_schema()
                      AND table_name = 'cross_signing_keys'
                      AND column_name = 'created_ts'
                ) THEN
                    added_ts_sources := added_ts_sources || ', created_ts';
                END IF;

                EXECUTE format(
                    'UPDATE cross_signing_keys
                     SET signatures = COALESCE(signatures, ''{}''::jsonb),
                         added_ts = COALESCE(%s, (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT)
                     WHERE signatures IS NULL OR added_ts IS NULL',
                    added_ts_sources
                );
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id)")
            .execute(&*self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_signatures (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                target_user_id TEXT NOT NULL,
                target_device_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                signature TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                CONSTRAINT uq_device_signatures_unique UNIQUE (user_id, device_id, target_user_id, target_device_id, algorithm)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create backup_keys table (密钥备份数据)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS backup_keys (
                id BIGSERIAL PRIMARY KEY,
                backup_id BIGINT NOT NULL,
                room_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                session_data JSONB NOT NULL,
                first_message_index BIGINT,
                forwarded_count BIGINT DEFAULT 0,
                is_verified BOOLEAN DEFAULT FALSE,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id)")
            .execute(&*self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id)")
            .execute(&*self.pool)
            .await?;

        Ok("E2EE核心表创建完成".to_string())
    }

    pub(crate) async fn step_ensure_additional_tables(&self) -> Result<String, sqlx::Error> {
        // Ensure typing table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS typing (
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                typing BOOLEAN DEFAULT FALSE,
                last_active_ts BIGINT NOT NULL,
                CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure search tables exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS search_index (
                id SERIAL PRIMARY KEY,
                event_id VARCHAR(255) NOT NULL,
                room_id VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                type VARCHAR(255) NOT NULL,
                content TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT uq_search_index_event UNIQUE (event_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_search_index_room ON search_index(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_search_index_user ON search_index(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_search_index_type ON search_index(event_type)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure user_directory table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_directory (
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                visibility TEXT NOT NULL DEFAULT 'private',
                added_by TEXT,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT pk_user_directory PRIMARY KEY (user_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_user_directory_user ON user_directory(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_user_directory_visibility ON user_directory(visibility)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure is_guest column exists in users table
        sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE")
            .execute(&*self.pool)
            .await?;

        // Ensure user_privacy_settings table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_privacy_settings (
                user_id VARCHAR(255) PRIMARY KEY,
                allow_presence_lookup BOOLEAN DEFAULT TRUE,
                allow_profile_lookup BOOLEAN DEFAULT TRUE,
                allow_room_invites BOOLEAN DEFAULT TRUE,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure pushers table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pushers (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                pushkey TEXT NOT NULL,
                pushkey_ts BIGINT NOT NULL,
                kind TEXT NOT NULL,
                app_id TEXT NOT NULL,
                app_display_name TEXT NOT NULL,
                device_display_name TEXT NOT NULL,
                profile_tag TEXT,
                lang TEXT DEFAULT 'en',
                data JSONB DEFAULT '{}',
                updated_ts BIGINT,
                created_ts BIGINT NOT NULL,
                is_enabled BOOLEAN DEFAULT TRUE,
                CONSTRAINT uq_pushers_user_device_pushkey UNIQUE (user_id, device_id, pushkey)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure account_data table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS account_data (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                data_type TEXT NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_account_data_user_type UNIQUE (user_id, data_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure key_backups table exists with backup_id column
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_backups (
                backup_id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                backup_id_text TEXT,
                algorithm TEXT NOT NULL,
                auth_data JSONB,
                auth_key TEXT,
                mgmt_key TEXT,
                version BIGINT DEFAULT 1,
                etag TEXT,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT,
                CONSTRAINT uq_key_backups_user_version UNIQUE (user_id, version)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure rooms table has guest_access column (for RoomSummary compatibility)
        // Note: rooms table has has_guest_access BOOLEAN, but room_summaries uses guest_access VARCHAR
        sqlx::query("ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden'")
            .execute(&*self.pool)
            .await?;

        // Ensure refresh_tokens table has expires_at column
        sqlx::query("ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at BIGINT")
            .execute(&*self.pool)
            .await?;

        sqlx::query("ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS is_fallback BOOLEAN DEFAULT FALSE")
            .execute(&*self.pool)
            .await?;

        // Ensure room_tags table exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_tags (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                room_id VARCHAR(255) NOT NULL,
                tag VARCHAR(255) NOT NULL,
                order_value DOUBLE PRECISION,
                created_ts BIGINT NOT NULL,
                UNIQUE (user_id, room_id, tag)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_tags_user ON room_tags(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure room_events table for event retrieval
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_events (
                id SERIAL PRIMARY KEY,
                event_id VARCHAR(255) UNIQUE NOT NULL,
                room_id VARCHAR(255) NOT NULL,
                sender VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                state_key VARCHAR(255),
                content JSONB NOT NULL DEFAULT '{}',
                prev_event_id VARCHAR(255),
                origin_server_ts BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_events_room ON room_events(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_events_event ON room_events(event_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure to_device_messages table for E2EE to-device messaging
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS to_device_messages (
                id SERIAL PRIMARY KEY,
                sender_user_id VARCHAR(255) NOT NULL,
                sender_device_id VARCHAR(255) NOT NULL,
                recipient_user_id VARCHAR(255) NOT NULL,
                recipient_device_id VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                message_id VARCHAR(255),
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_to_device_recipient ON to_device_messages(recipient_user_id, recipient_device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_to_device_stream ON to_device_messages(recipient_user_id, stream_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS to_device_transactions (
                id SERIAL PRIMARY KEY,
                sender_user_id VARCHAR(255) NOT NULL,
                sender_device_id VARCHAR(255) NOT NULL,
                message_id VARCHAR(255) NOT NULL,
                created_ts BIGINT NOT NULL,
                UNIQUE (sender_user_id, sender_device_id, message_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure device_lists_changes table for tracking device list updates
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_lists_changes (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                change_type VARCHAR(50) NOT NULL,
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_lists_user ON device_lists_changes(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_lists_stream ON device_lists_changes(stream_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure room_ephemeral table for typing, receipts, etc.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_ephemeral (
                id SERIAL PRIMARY KEY,
                room_id VARCHAR(255) NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_ephemeral_room ON room_ephemeral(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_room_ephemeral_room_type_user
            ON room_ephemeral(room_id, event_type, user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure device_lists_stream table for tracking device list stream position
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS device_lists_stream (
                stream_id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_device_lists_stream_user ON device_lists_stream(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure user_filters table for filter persistence
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_filters (
                id SERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                filter_id VARCHAR(255) NOT NULL,
                filter_json JSONB NOT NULL DEFAULT '{}',
                created_ts BIGINT NOT NULL,
                UNIQUE (user_id, filter_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_user_filters_user ON user_filters(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure room_account_data table for per-room account data
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS room_account_data (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                data_type TEXT NOT NULL,
                content JSONB NOT NULL DEFAULT '{}',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_room_account_data UNIQUE (user_id, room_id, data_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_account_data_user ON room_account_data(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_room_account_data_user_room ON room_account_data(user_id, room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure read_markers table for unread counts
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS read_markers (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                CONSTRAINT uq_read_markers UNIQUE (user_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_read_markers_user ON read_markers(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_read_markers_user_room ON read_markers(user_id, room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure key_rotation_pending table for E2EE key rotation
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_rotation_pending (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                rotation_reason TEXT,
                created_ts BIGINT NOT NULL,
                processed BOOLEAN NOT NULL DEFAULT FALSE
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_key_rotation_pending_user ON key_rotation_pending(user_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_key_rotation_pending_unprocessed ON key_rotation_pending(user_id) WHERE processed = FALSE
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure key_rotation_state table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_rotation_state (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                rotation_count BIGINT NOT NULL DEFAULT 0,
                last_rotation_ts BIGINT,
                CONSTRAINT uq_key_rotation_state UNIQUE (user_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure key_rotation_config table for persisted rotation parameters
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_rotation_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure lazy_loaded_members table for sync optimization
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS lazy_loaded_members (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                member_user_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                CONSTRAINT uq_lazy_loaded_members UNIQUE (user_id, room_id, member_user_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_lazy_loaded_members_user_room ON lazy_loaded_members(user_id, room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure sync_stream_id sequence table for generating stream IDs
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_stream_id (
                id BIGSERIAL PRIMARY KEY,
                stream_type TEXT,
                last_id BIGINT DEFAULT 0,
                updated_ts BIGINT,
                CONSTRAINT uq_sync_stream_id_type UNIQUE (stream_type)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Ensure a row exists for generating stream IDs
        sqlx::query(
            r#"
            INSERT INTO sync_stream_id (id) VALUES (1) ON CONFLICT DO NOTHING
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query("CREATE SEQUENCE IF NOT EXISTS sliding_sync_pos_seq").execute(&*self.pool).await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sliding_sync_lists (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                conn_id TEXT,
                list_key TEXT NOT NULL,
                sort JSONB DEFAULT '[]',
                filters JSONB DEFAULT '{}',
                room_subscription JSONB DEFAULT '{}',
                ranges JSONB DEFAULT '[]',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_lists_unique ON sliding_sync_lists(user_id, device_id, COALESCE(conn_id, ''), list_key)",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sliding_sync_lists_user_device ON sliding_sync_lists(user_id, device_id)",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sliding_sync_tokens (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                conn_id TEXT,
                token TEXT NOT NULL,
                pos BIGINT NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_tokens_unique ON sliding_sync_tokens(user_id, device_id, COALESCE(conn_id, ''))",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_user ON sliding_sync_tokens(user_id, device_id)",
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sliding_sync_rooms (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                conn_id TEXT,
                list_key TEXT,
                bump_stamp BIGINT DEFAULT 0,
                highlight_count INTEGER DEFAULT 0,
                notification_count INTEGER DEFAULT 0,
                is_dm BOOLEAN DEFAULT FALSE,
                is_encrypted BOOLEAN DEFAULT FALSE,
                is_tombstoned BOOLEAN DEFAULT FALSE,
                is_invited BOOLEAN DEFAULT FALSE,
                name TEXT,
                avatar TEXT,
                timestamp BIGINT DEFAULT 0,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create unique index for sliding_sync_rooms (using COALESCE in index)
        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_sliding_sync_rooms_unique ON sliding_sync_rooms (user_id, device_id, room_id, COALESCE(conn_id, ''))
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_user_device ON sliding_sync_rooms(user_id, device_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_bump_stamp ON sliding_sync_rooms(bump_stamp DESC)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_sliding_sync_rooms_room_id ON sliding_sync_rooms(room_id, updated_ts DESC)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create thread_subscriptions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS thread_subscriptions (
                id BIGSERIAL PRIMARY KEY,
                room_id TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                notification_level TEXT DEFAULT 'all',
                is_muted BOOLEAN DEFAULT FALSE,
                is_pinned BOOLEAN DEFAULT FALSE,
                subscribed_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                UNIQUE (room_id, thread_id, user_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_thread_subscriptions_room_thread ON thread_subscriptions(room_id, thread_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create space_children table (with all fields including those added via migration)
        // First ensure the table exists, then add any missing columns
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS space_children (
                id BIGSERIAL PRIMARY KEY,
                space_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                sender TEXT NOT NULL,
                is_suggested BOOLEAN DEFAULT FALSE,
                via_servers JSONB DEFAULT '[]',
                added_ts BIGINT NOT NULL,
                CONSTRAINT pk_space_children PRIMARY KEY (id),
                CONSTRAINT uq_space_children_space_room UNIQUE (space_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Add missing columns if they don't exist (for databases created before this migration)
        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'order'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN "order" TEXT DEFAULT '';
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'suggested'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN suggested BOOLEAN DEFAULT FALSE;
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'added_by'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN added_by TEXT DEFAULT '';
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = 'space_children'
                    AND column_name = 'removed_ts'
                ) THEN
                    ALTER TABLE space_children ADD COLUMN removed_ts BIGINT;
                END IF;
            END $$;
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Create space_hierarchy table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS space_hierarchy (
                id BIGSERIAL PRIMARY KEY,
                space_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                parent_space_id TEXT,
                depth INTEGER DEFAULT 0,
                children TEXT[],
                via_servers TEXT[],
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                UNIQUE (space_id, room_id)
            )
            "#,
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_space_hierarchy_space ON space_hierarchy(space_id)
            "#,
        )
        .execute(&*self.pool)
        .await?;

        // Seed default captcha templates if not present
        let captcha_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM captcha_template").fetch_one(&*self.pool).await.unwrap_or(0);
        if captcha_count == 0 {
            let now_ts = current_timestamp_millis();
            if let Err(e) = sqlx::query(
                r#"
                INSERT INTO captcha_template (template_name, captcha_type, subject, content, is_default, is_enabled, created_ts, updated_ts)
                VALUES
                    ('default_email', 'email', 'Verification Code', 'Your verification code is {{code}}, valid for {{expiry_minutes}} minutes.', true, true, $1, $1),
                    ('default_sms', 'sms', '', 'Your verification code is {{code}}, valid for {{expiry_minutes}} minutes.', true, true, $1, $1),
                    ('default_image', 'image', '', 'Your verification code is {{code}}, valid for {{expiry_minutes}} minutes.', true, true, $1, $1)
                "#,
            )
            .bind(now_ts)
            .execute(&*self.pool)
            .await
            {
                warn!(error = %e, seeded_templates = 3_u64, "Failed to seed captcha templates");
            }
        }

        Ok("附加表和列检查完成".to_string())
    }
}
