-- Migration: Align database schema with runtime storage models and admin APIs
-- Version: 20260304000001
-- Date: 2026-03-04

-- ============================================================================
-- 1. Rooms 时间字段兼容（created_ts / created_ts）
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE rooms ADD COLUMN created_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE rooms ADD COLUMN created_ts BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms' AND column_name = 'created_ts'
    ) THEN
        UPDATE rooms
        SET created_ts = COALESCE(created_ts, created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
            created_ts = COALESCE(created_ts, created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT);
    END IF;
END $$;

-- ============================================================================
-- 2. application_services 主表与关联表补齐
-- ============================================================================
CREATE TABLE IF NOT EXISTS application_services (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    as_token TEXT NOT NULL,
    hs_token TEXT NOT NULL,
    sender TEXT NOT NULL,
    name TEXT,
    description TEXT,
    rate_limited BOOLEAN NOT NULL DEFAULT FALSE,
    protocols JSONB NOT NULL DEFAULT '[]'::jsonb,
    namespaces JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_seen_ts BIGINT,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE
);

ALTER TABLE application_services ADD COLUMN IF NOT EXISTS as_id TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS sender TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS url TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS as_token TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS hs_token TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS rate_limited BOOLEAN DEFAULT FALSE;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS protocols JSONB DEFAULT '[]'::jsonb;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS namespaces JSONB DEFAULT '{}'::jsonb;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS updated_ts BIGINT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS last_seen_ts BIGINT;
ALTER TABLE application_services ADD COLUMN IF NOT EXISTS is_enabled BOOLEAN DEFAULT TRUE;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'sender_localpart'
    ) THEN
        UPDATE application_services
        SET sender = COALESCE(sender, sender_localpart)
        WHERE sender IS NULL;
    END IF;
END $$;

UPDATE application_services
SET as_id = COALESCE(as_id, CONCAT('as_', id::TEXT))
WHERE as_id IS NULL;

UPDATE application_services
SET url = COALESCE(url, ''),
    as_token = COALESCE(as_token, ''),
    hs_token = COALESCE(hs_token, ''),
    sender = COALESCE(sender, ''),
    created_ts = COALESCE(created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    rate_limited = COALESCE(rate_limited, FALSE),
    protocols = COALESCE(protocols, '[]'::jsonb),
    namespaces = COALESCE(namespaces, '{}'::jsonb),
    is_enabled = COALESCE(is_enabled, TRUE)
WHERE url IS NULL
   OR as_token IS NULL
   OR hs_token IS NULL
   OR sender IS NULL
   OR created_ts IS NULL
   OR rate_limited IS NULL
   OR protocols IS NULL
   OR namespaces IS NULL
   OR is_enabled IS NULL;

ALTER TABLE application_services ALTER COLUMN as_id SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN url SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN as_token SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN hs_token SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN sender SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN created_ts SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN rate_limited SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN protocols SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN namespaces SET NOT NULL;
ALTER TABLE application_services ALTER COLUMN is_enabled SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_application_services_as_id ON application_services(as_id);
CREATE INDEX IF NOT EXISTS idx_application_services_token ON application_services(as_token);
CREATE INDEX IF NOT EXISTS idx_application_services_enabled ON application_services(is_enabled);

CREATE TABLE IF NOT EXISTS application_service_state (
    as_id TEXT NOT NULL,
    state_key TEXT NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL,
    PRIMARY KEY (as_id, state_key),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_service_events (
    event_id TEXT PRIMARY KEY,
    as_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    transaction_id TEXT,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

ALTER TABLE application_service_events ADD COLUMN IF NOT EXISTS as_id TEXT;
ALTER TABLE application_service_events ADD COLUMN IF NOT EXISTS sender TEXT;
ALTER TABLE application_service_events ADD COLUMN IF NOT EXISTS content JSONB;
ALTER TABLE application_service_events ADD COLUMN IF NOT EXISTS state_key TEXT;
ALTER TABLE application_service_events ADD COLUMN IF NOT EXISTS origin_server_ts BIGINT;
ALTER TABLE application_service_events ADD COLUMN IF NOT EXISTS transaction_id TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events' AND column_name = 'appservice_id'
    ) THEN
        UPDATE application_service_events
        SET as_id = COALESCE(as_id, appservice_id)
        WHERE as_id IS NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events' AND column_name = 'created_ts'
    ) THEN
        UPDATE application_service_events
        SET origin_server_ts = COALESCE(origin_server_ts, created_ts)
        WHERE origin_server_ts IS NULL;
    END IF;
END $$;

UPDATE application_service_events
SET sender = COALESCE(sender, ''),
    content = COALESCE(content, '{}'::jsonb),
    origin_server_ts = COALESCE(origin_server_ts, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
WHERE sender IS NULL
   OR content IS NULL
   OR origin_server_ts IS NULL;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events' AND column_name = 'as_id'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events' AND column_name = 'origin_server_ts'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_as_events_pending ON application_service_events(as_id, origin_server_ts)
        WHERE processed_ts IS NULL;
    ELSIF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events' AND column_name = 'appservice_id'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events' AND column_name = 'created_ts'
    ) THEN
        CREATE INDEX IF NOT EXISTS idx_as_events_pending_legacy ON application_service_events(appservice_id, created_ts)
        WHERE processed_ts IS NULL;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    events JSONB NOT NULL,
    sent_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    UNIQUE(as_id, transaction_id),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_as_transactions_pending
ON application_service_transactions(as_id, sent_ts)
WHERE completed_ts IS NULL;

CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    exclusive BOOLEAN NOT NULL DEFAULT FALSE,
    regex TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

ALTER TABLE application_service_user_namespaces ADD COLUMN IF NOT EXISTS as_id TEXT;
ALTER TABLE application_service_user_namespaces ADD COLUMN IF NOT EXISTS namespace_pattern TEXT;
ALTER TABLE application_service_user_namespaces ADD COLUMN IF NOT EXISTS exclusive BOOLEAN DEFAULT FALSE;
ALTER TABLE application_service_user_namespaces ADD COLUMN IF NOT EXISTS regex TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_user_namespaces' AND column_name = 'appservice_id'
    ) THEN
        UPDATE application_service_user_namespaces
        SET as_id = COALESCE(as_id, appservice_id)
        WHERE as_id IS NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_user_namespaces' AND column_name = 'pattern'
    ) THEN
        UPDATE application_service_user_namespaces
        SET namespace_pattern = COALESCE(namespace_pattern, pattern),
            regex = COALESCE(regex, pattern)
        WHERE namespace_pattern IS NULL OR regex IS NULL;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    exclusive BOOLEAN NOT NULL DEFAULT FALSE,
    regex TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    exclusive BOOLEAN NOT NULL DEFAULT FALSE,
    regex TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_service_users (
    as_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    displayname TEXT,
    avatar_url TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (as_id, user_id),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

ALTER TABLE application_service_users ADD COLUMN IF NOT EXISTS as_id TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_users' AND column_name = 'appservice_id'
    ) THEN
        UPDATE application_service_users
        SET as_id = COALESCE(as_id, appservice_id)
        WHERE as_id IS NULL;
    END IF;
END $$;

DO $$
DECLARE
    rel_kind CHAR;
BEGIN
    SELECT c.relkind
    INTO rel_kind
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'public' AND c.relname = 'application_service_statistics'
    LIMIT 1;

    IF rel_kind = 'v' THEN
        EXECUTE 'DROP VIEW IF EXISTS application_service_statistics';
    ELSIF rel_kind = 'r' THEN
        EXECUTE 'DROP TABLE IF EXISTS application_service_statistics';
    END IF;
END $$;

CREATE OR REPLACE VIEW application_service_statistics AS
SELECT
    s.id,
    s.as_id,
    s.name,
    s.is_enabled,
    s.rate_limited,
    COALESCE(u.virtual_user_count, 0) AS virtual_user_count,
    COALESCE(e.pending_event_count, 0) AS pending_event_count,
    COALESCE(t.pending_transaction_count, 0) AS pending_transaction_count,
    s.last_seen_ts,
    s.created_ts
FROM application_services s
LEFT JOIN (
    SELECT as_id, COUNT(*)::BIGINT AS virtual_user_count
    FROM application_service_users
    GROUP BY as_id
) u ON u.as_id = s.as_id
LEFT JOIN (
    SELECT as_id, COUNT(*)::BIGINT AS pending_event_count
    FROM application_service_events
    WHERE processed_ts IS NULL
    GROUP BY as_id
) e ON e.as_id = s.as_id
LEFT JOIN (
    SELECT as_id, COUNT(*)::BIGINT AS pending_transaction_count
    FROM application_service_transactions
    WHERE completed_ts IS NULL
    GROUP BY as_id
) t ON t.as_id = s.as_id;

-- ============================================================================
-- 3. background_updates 体系对齐（含锁表、历史表）
-- ============================================================================
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS job_name TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS job_type TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS table_name TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS column_name TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS status TEXT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS total_items INTEGER DEFAULT 0;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS processed_items INTEGER DEFAULT 0;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS last_updated_ts BIGINT;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS retry_count INTEGER DEFAULT 0;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS max_retries INTEGER DEFAULT 3;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS batch_size INTEGER DEFAULT 100;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS sleep_ms INTEGER DEFAULT 1000;
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS depends_on TEXT[];
ALTER TABLE background_updates ADD COLUMN IF NOT EXISTS metadata JSONB;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'update_name'
    ) THEN
        UPDATE background_updates
        SET job_name = COALESCE(job_name, update_name)
        WHERE job_name IS NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'id'
    ) THEN
        UPDATE background_updates
        SET job_name = COALESCE(job_name, CONCAT('job_', id::TEXT))
        WHERE job_name IS NULL;
    ELSE
        UPDATE background_updates
        SET job_name = COALESCE(job_name, CONCAT('job_', SUBSTRING(MD5(ctid::TEXT), 1, 8)))
        WHERE job_name IS NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'is_running'
    ) THEN
        UPDATE background_updates
        SET status = COALESCE(status, CASE WHEN is_running THEN 'running' ELSE 'pending' END)
        WHERE status IS NULL;
    ELSE
        UPDATE background_updates
        SET status = COALESCE(status, 'pending')
        WHERE status IS NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'started_ts'
    ) THEN
        UPDATE background_updates
        SET created_ts = COALESCE(created_ts, started_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
        WHERE created_ts IS NULL;
    ELSE
        UPDATE background_updates
        SET created_ts = COALESCE(created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
        WHERE created_ts IS NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'completed_ts'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'started_ts'
    ) THEN
        UPDATE background_updates
        SET last_updated_ts = COALESCE(last_updated_ts, completed_ts, started_ts, created_ts)
        WHERE last_updated_ts IS NULL;
    ELSIF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'background_updates' AND column_name = 'started_ts'
    ) THEN
        UPDATE background_updates
        SET last_updated_ts = COALESCE(last_updated_ts, started_ts, created_ts)
        WHERE last_updated_ts IS NULL;
    ELSE
        UPDATE background_updates
        SET last_updated_ts = COALESCE(last_updated_ts, created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
        WHERE last_updated_ts IS NULL;
    END IF;

    UPDATE background_updates
    SET metadata = COALESCE(metadata, '{}'::jsonb)
    WHERE metadata IS NULL;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_background_updates_job_name
ON background_updates(job_name);
CREATE INDEX IF NOT EXISTS idx_background_updates_status_created
ON background_updates(status, created_ts);

CREATE TABLE IF NOT EXISTS background_update_locks (
    job_name TEXT PRIMARY KEY,
    locked_by TEXT,
    locked_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_background_update_locks_expires
ON background_update_locks(expires_ts);

CREATE TABLE IF NOT EXISTS background_update_history (
    id BIGSERIAL PRIMARY KEY,
    job_name TEXT NOT NULL,
    execution_start_ts BIGINT NOT NULL,
    execution_end_ts BIGINT,
    status TEXT NOT NULL,
    items_processed INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    metadata JSONB,
    FOREIGN KEY (job_name) REFERENCES background_updates(job_name) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_background_update_history_job_start
ON background_update_history(job_name, execution_start_ts DESC);

-- ============================================================================
-- 4. Room Summary 体系补齐（成员、状态、统计、队列）
-- ============================================================================
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS id BIGSERIAL;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS room_type TEXT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS avatar_url TEXT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS join_rules TEXT DEFAULT 'invite';
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS history_visibility TEXT DEFAULT 'shared';
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS guest_access TEXT DEFAULT 'forbidden';
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS is_direct BOOLEAN DEFAULT FALSE;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS is_space BOOLEAN DEFAULT FALSE;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS is_encrypted BOOLEAN DEFAULT FALSE;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS member_count INTEGER DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS joined_member_count INTEGER DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS invited_member_count INTEGER DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS last_event_id TEXT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS last_event_ts BIGINT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS last_message_ts BIGINT;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS unread_notifications INTEGER DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS unread_highlight INTEGER DEFAULT 0;
ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS created_ts BIGINT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'room_summaries' AND column_name = 'joined_members'
    ) THEN
        UPDATE room_summaries
        SET joined_member_count = COALESCE(joined_member_count, joined_members::INTEGER, 0)
        WHERE joined_member_count IS NULL;
    ELSE
        UPDATE room_summaries
        SET joined_member_count = COALESCE(joined_member_count, 0)
        WHERE joined_member_count IS NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'room_summaries' AND column_name = 'invited_members'
    ) THEN
        UPDATE room_summaries
        SET invited_member_count = COALESCE(invited_member_count, invited_members::INTEGER, 0)
        WHERE invited_member_count IS NULL;
    ELSE
        UPDATE room_summaries
        SET invited_member_count = COALESCE(invited_member_count, 0)
        WHERE invited_member_count IS NULL;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'room_summaries' AND column_name = 'can_guest_join'
    ) THEN
        UPDATE room_summaries
        SET guest_access = COALESCE(guest_access, CASE WHEN can_guest_join THEN 'can_join' ELSE 'forbidden' END)
        WHERE guest_access IS NULL;
    ELSE
        UPDATE room_summaries
        SET guest_access = COALESCE(guest_access, 'forbidden')
        WHERE guest_access IS NULL;
    END IF;

    UPDATE room_summaries
    SET member_count = COALESCE(member_count, joined_member_count + invited_member_count, 0),
        join_rules = COALESCE(join_rules, 'invite'),
        history_visibility = COALESCE(history_visibility, 'shared'),
        created_ts = COALESCE(created_ts, updated_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
        updated_ts = COALESCE(updated_ts, created_ts, (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
        hero_users = COALESCE(hero_users, '[]'::jsonb),
        unread_notifications = COALESCE(unread_notifications, 0),
        unread_highlight = COALESCE(unread_highlight, 0),
        is_direct = COALESCE(is_direct, FALSE),
        is_space = COALESCE(is_space, FALSE),
        is_encrypted = COALESCE(is_encrypted, FALSE)
    WHERE member_count IS NULL
       OR join_rules IS NULL
       OR history_visibility IS NULL
       OR created_ts IS NULL
       OR updated_ts IS NULL
       OR hero_users IS NULL
       OR unread_notifications IS NULL
       OR unread_highlight IS NULL
       OR is_direct IS NULL
       OR is_space IS NULL
       OR is_encrypted IS NULL;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_room_summaries_id_unique ON room_summaries(id);

CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    membership TEXT NOT NULL,
    is_hero BOOLEAN NOT NULL DEFAULT FALSE,
    last_active_ts BIGINT,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room
ON room_summary_members(room_id);

CREATE TABLE IF NOT EXISTS room_summary_state (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    event_id TEXT,
    content JSONB NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(room_id, event_type, state_key)
);

CREATE INDEX IF NOT EXISTS idx_room_summary_state_room
ON room_summary_state(room_id);

CREATE TABLE IF NOT EXISTS room_summary_stats (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    total_events BIGINT NOT NULL DEFAULT 0,
    total_state_events BIGINT NOT NULL DEFAULT 0,
    total_messages BIGINT NOT NULL DEFAULT 0,
    total_media BIGINT NOT NULL DEFAULT 0,
    storage_size BIGINT NOT NULL DEFAULT 0,
    last_updated_ts BIGINT NOT NULL
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
    retry_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_pending
ON room_summary_update_queue(status, priority DESC, created_ts ASC);
