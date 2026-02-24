-- =============================================================================
-- Synapse-Rust 索引效果验证脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-27
-- 描述: 使用 EXPLAIN ANALYZE 验证索引效果
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260227000001_verify_indexes.sql
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 验证说明
-- =============================================================================
-- 查看执行计划时，关注以下指标：
-- 1. Index Scan vs Sequential Scan - 索引扫描优于全表扫描
-- 2. Index Cond - 索引条件是否被使用
-- 3. Rows Removed by Filter - 被过滤掉的行数（越少越好）
-- 4. Execution Time - 执行时间（毫秒）
-- 5. Buffers - 缓冲区读取次数（shared read 越少越好）

\echo '========================================'
\echo '索引效果验证开始'
\echo '========================================'

-- =============================================================================
-- 第一部分: events 表查询验证
-- =============================================================================

\echo ''
\echo '--- 1. 房间事件按时间排序查询 ---'
\echo '查询: WHERE room_id = $1 ORDER BY origin_server_ts DESC'
\echo '预期: 使用 idx_events_room_origin_ts 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT event_id, room_id, user_id, event_type, content
FROM events 
WHERE room_id = '!test:example.com'
ORDER BY origin_server_ts DESC
LIMIT 100;

\echo ''
\echo '--- 2. 房间事件按类型和时间查询 ---'
\echo '查询: WHERE room_id = $1 AND event_type = $2 ORDER BY origin_server_ts DESC'
\echo '预期: 使用 idx_events_room_type_origin_ts 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT event_id, room_id, user_id, event_type, content
FROM events 
WHERE room_id = '!test:example.com' AND event_type = 'm.room.message'
ORDER BY origin_server_ts DESC
LIMIT 100;

\echo ''
\echo '--- 3. 房间状态事件查询 ---'
\echo '查询: WHERE room_id = $1 AND state_key IS NOT NULL'
\echo '预期: 使用 idx_events_room_state_origin_ts 部分索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT event_id, room_id, sender, event_type, content, state_key
FROM events 
WHERE room_id = '!test:example.com' AND state_key IS NOT NULL
ORDER BY origin_server_ts DESC
LIMIT 100;

\echo ''
\echo '--- 4. 增量同步查询 ---'
\echo '查询: WHERE room_id = $1 AND origin_server_ts > $2 ORDER BY origin_server_ts ASC'
\echo '预期: 使用 idx_events_room_since_ts 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT event_id, room_id, user_id, event_type, content
FROM events 
WHERE room_id = '!test:example.com' AND origin_server_ts > 1700000000000
ORDER BY origin_server_ts ASC
LIMIT 100;

\echo ''
\echo '--- 5. 消息统计查询 ---'
\echo '查询: WHERE room_id = $1 AND event_type = m.room.message'
\echo '预期: 使用 idx_events_room_messages 部分索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT COUNT(*) 
FROM events 
WHERE room_id = '!test:example.com' AND event_type = 'm.room.message';

-- =============================================================================
-- 第二部分: users 表查询验证
-- =============================================================================

\echo ''
\echo '--- 6. 用户名查询 ---'
\echo '查询: WHERE username = $1'
\echo '预期: 使用 idx_users_username 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT user_id, username, displayname, avatar_url
FROM users 
WHERE username = 'testuser';

\echo ''
\echo '--- 7. 活跃用户查询 ---'
\echo '查询: WHERE is_deactivated = FALSE'
\echo '预期: 使用 idx_users_active 部分索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT user_id, username, displayname
FROM users 
WHERE COALESCE(is_deactivated, FALSE) = FALSE
LIMIT 100;

\echo ''
\echo '--- 8. 用户搜索查询 ---'
\echo '查询: WHERE username ILIKE $1 OR displayname ILIKE $1'
\echo '预期: 使用 idx_users_search GIN 索引（需要 pg_trgm 扩展）'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT user_id, username, displayname, avatar_url
FROM users 
WHERE username ILIKE '%test%' OR displayname ILIKE '%test%'
LIMIT 20;

-- =============================================================================
-- 第三部分: room_memberships 表查询验证
-- =============================================================================

\echo ''
\echo '--- 9. 房间成员按状态查询 ---'
\echo '查询: WHERE room_id = $1 AND membership = $2'
\echo '预期: 使用 idx_room_memberships_room_status 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT room_id, user_id, membership, display_name
FROM room_memberships 
WHERE room_id = '!test:example.com' AND membership = 'join';

\echo ''
\echo '--- 10. 用户加入的房间 ---'
\echo '查询: WHERE user_id = $1 AND membership = join'
\echo '预期: 使用 idx_room_memberships_user_joined 部分索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT room_id, user_id, membership
FROM room_memberships 
WHERE user_id = '@test:example.com' AND membership = 'join';

\echo ''
\echo '--- 11. 共享房间查询 ---'
\echo '查询: JOIN 判断两用户是否共享房间'
\echo '预期: 使用 idx_room_memberships_user_membership 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT 1 
FROM room_memberships m1
JOIN room_memberships m2 ON m1.room_id = m2.room_id
WHERE m1.user_id = '@user1:example.com' AND m1.membership = 'join'
  AND m2.user_id = '@user2:example.com' AND m2.membership = 'join'
LIMIT 1;

-- =============================================================================
-- 第四部分: access_tokens 表查询验证
-- =============================================================================

\echo ''
\echo '--- 12. 令牌验证查询 ---'
\echo '查询: WHERE token = $1'
\echo '预期: 使用 idx_access_tokens_token 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT id, token, user_id, device_id, is_valid
FROM access_tokens 
WHERE token = 'test_token_123';

\echo ''
\echo '--- 13. 用户有效令牌查询 ---'
\echo '查询: WHERE user_id = $1 AND is_valid = TRUE'
\echo '预期: 使用 idx_access_tokens_user_valid 部分索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT id, token, user_id, device_id
FROM access_tokens 
WHERE user_id = '@test:example.com' AND is_valid = TRUE;

-- =============================================================================
-- 第五部分: devices 表查询验证
-- =============================================================================

\echo ''
\echo '--- 14. 用户设备列表查询 ---'
\echo '查询: WHERE user_id = $1 ORDER BY last_seen_ts DESC'
\echo '预期: 使用 idx_devices_user_last_seen 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT device_id, user_id, display_name, last_seen_ts
FROM devices 
WHERE user_id = '@test:example.com'
ORDER BY last_seen_ts DESC;

-- =============================================================================
-- 第六部分: push_notification_queue 表查询验证
-- =============================================================================

\echo ''
\echo '--- 15. 待发送推送队列查询 ---'
\echo '查询: WHERE status = pending ORDER BY next_attempt_at'
\echo '预期: 使用 idx_push_queue_pending 部分索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT id, user_id, device_id, event_id, content
FROM push_notification_queue 
WHERE status = 'pending'
ORDER BY next_attempt_at
LIMIT 100;

-- =============================================================================
-- 第七部分: event_reports 表查询验证
-- =============================================================================

\echo ''
\echo '--- 16. 事件举报查询 ---'
\echo '查询: WHERE event_id = $1 ORDER BY received_ts DESC'
\echo '预期: 使用 idx_event_reports_event_ts 索引'

EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT id, event_id, room_id, reporter_user_id, reason, score
FROM event_reports 
WHERE event_id = '$event123:example.com'
ORDER BY received_ts DESC;

-- =============================================================================
-- 第八部分: 索引统计信息
-- =============================================================================

\echo ''
\echo '========================================'
\echo '索引统计信息'
\echo '========================================'

\echo ''
\echo '--- 所有自定义索引列表 ---'
SELECT 
    schemaname,
    tablename,
    indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_indexes 
JOIN pg_class ON pg_class.oid = indexrelid
WHERE schemaname = 'public' 
AND indexname LIKE 'idx_%'
ORDER BY tablename, indexname;

\echo ''
\echo '--- 索引使用统计 ---'
SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan as index_scans,
    idx_tup_read as tuples_read,
    idx_tup_fetch as tuples_fetched,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes 
JOIN pg_class ON pg_class.oid = indexrelid
WHERE schemaname = 'public'
AND indexname LIKE 'idx_%'
ORDER BY idx_scan DESC
LIMIT 30;

\echo ''
\echo '--- 未使用的索引 ---'
SELECT 
    schemaname,
    tablename,
    indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes 
JOIN pg_class ON pg_class.oid = indexrelid
WHERE schemaname = 'public'
AND indexname LIKE 'idx_%'
AND idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;

\echo ''
\echo '========================================'
\echo '索引效果验证完成'
\echo '========================================'
