-- ============================================================================
-- Forward Script: 20260515000007_rooms_summaries_materialized_view_v7.sql
-- Description: Creates materialized view for room summaries to accelerate
--              public room directory, admin dashboards, and room list queries.
--              组合 rooms、room_memberships、events 三表数据，减少重复 JOIN。
-- Created: 2026-05-09
-- Risk: LOW — 纯增量视图，不影响现有数据和查询。
-- Refresh: CONCURRENTLY，建议通过 pg_cron 定时刷新（如每 5 分钟）。
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 1. 物化视图: rooms_summaries
--    缓存每个房间的成员计数、最近活跃时间、名称/主题/头像等摘要信息。
-- ============================================================================
CREATE MATERIALIZED VIEW IF NOT EXISTS rooms_summaries AS
SELECT
    r.room_id,
    r.creator,
    r.room_version,
    r.join_rules,
    r.is_public,
    r.history_visibility,
    r.created_ts,
    r.last_activity_ts,
    COALESCE(member_count.cnt, 0)                       AS joined_members,
    COALESCE(member_count.cnt, 0) +
        COALESCE(invite_count.cnt, 0)                   AS total_members,
    COALESCE(invite_count.cnt, 0)                       AS invited_members,
    latest_event.event_id                               AS last_event_id,
    latest_event.event_type                             AS last_event_type,
    latest_event.sender                                 AS last_event_sender,
    latest_event.origin_server_ts                       AS last_event_ts,
    room_name.content->>'name'                          AS room_name,
    room_topic.content->>'topic'                        AS room_topic,
    room_avatar.content->>'url'                         AS room_avatar_url,
    room_canonical_alias.content->>'alias'              AS canonical_alias
FROM rooms r
LEFT JOIN LATERAL (
    SELECT COUNT(*) AS cnt
    FROM room_memberships
    WHERE room_id = r.room_id AND membership = 'join'
) member_count ON true
LEFT JOIN LATERAL (
    SELECT COUNT(*) AS cnt
    FROM room_memberships
    WHERE room_id = r.room_id AND membership = 'invite'
) invite_count ON true
LEFT JOIN LATERAL (
    SELECT event_id, event_type, sender, origin_server_ts
    FROM events
    WHERE room_id = r.room_id
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) latest_event ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.name'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_name ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.topic'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_topic ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.avatar'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_avatar ON true
LEFT JOIN LATERAL (
    SELECT content
    FROM events
    WHERE room_id = r.room_id
      AND event_type = 'm.room.canonical_alias'
      AND is_redacted = false
    ORDER BY origin_server_ts DESC
    LIMIT 1
) room_canonical_alias ON true;

-- ============================================================================
-- 2. 物化视图索引（支持高效查询）
-- ============================================================================
CREATE UNIQUE INDEX IF NOT EXISTS idx_rooms_summaries_room_id
    ON rooms_summaries (room_id);

CREATE INDEX IF NOT EXISTS idx_rooms_summaries_public_activity
    ON rooms_summaries (is_public, joined_members DESC, last_activity_ts DESC)
    WHERE is_public = true;

CREATE INDEX IF NOT EXISTS idx_rooms_summaries_creator
    ON rooms_summaries (creator, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_rooms_summaries_members
    ON rooms_summaries (joined_members DESC, last_activity_ts DESC);

-- ============================================================================
-- 3. 定时刷新函数（需 pg_cron 扩展）
-- ============================================================================
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'pg_cron'
    ) THEN
        PERFORM cron.schedule(
            'refresh-rooms-summaries',
            '*/5 * * * *',
            'REFRESH MATERIALIZED VIEW CONCURRENTLY rooms_summaries'
        );
    END IF;
END $$;

-- ============================================================================
-- 4. 物化视图: public_room_directory
--    基于 rooms_summaries 的公共房间目录视图，过滤掉非公开房间。
-- ============================================================================
CREATE MATERIALIZED VIEW IF NOT EXISTS public_room_directory AS
SELECT
    room_id,
    room_name,
    room_topic,
    room_avatar_url,
    canonical_alias,
    joined_members,
    total_members,
    last_event_ts,
    room_version,
    join_rules,
    history_visibility,
    created_ts
FROM rooms_summaries
WHERE is_public = true
  AND join_rules != 'knock';

CREATE UNIQUE INDEX IF NOT EXISTS idx_public_room_directory_room_id
    ON public_room_directory (room_id);

CREATE INDEX IF NOT EXISTS idx_public_room_directory_members
    ON public_room_directory (joined_members DESC, last_event_ts DESC);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'pg_cron'
    ) THEN
        PERFORM cron.schedule(
            'refresh-public-room-directory',
            '*/5 * * * *',
            'REFRESH MATERIALIZED VIEW CONCURRENTLY public_room_directory'
        );
    END IF;
END $$;

-- ============================================================================
-- Migration record
-- ============================================================================
INSERT INTO schema_migrations (version, name, is_success, description, applied_ts)
VALUES (
    '20260515000007',
    'rooms_summaries_materialized_view_v7',
    TRUE,
    'Create rooms_summaries and public_room_directory materialized views with concurrent refresh support',
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
)
ON CONFLICT (version) DO NOTHING;
