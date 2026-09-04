-- Migration undo: 20260904030000_schema_p3_perf.sql

-- P3-1: push_notification_queue 索引
DROP INDEX CONCURRENTLY IF EXISTS idx_push_queue_user_pending;
DROP INDEX CONCURRENTLY IF EXISTS idx_push_queue_retry;

-- P3-2: federation_queue 索引
DROP INDEX CONCURRENTLY IF EXISTS idx_federation_queue_dest_created;
DROP INDEX CONCURRENTLY IF EXISTS idx_federation_queue_retry;

-- P3-3: backup_keys 约束
ALTER TABLE backup_keys DROP CONSTRAINT IF EXISTS uq_backup_keys_room_session;
ALTER TABLE backup_keys DROP CONSTRAINT IF EXISTS fk_backup_keys_room;

-- P3-4: rooms.is_federated 索引
DROP INDEX CONCURRENTLY IF EXISTS idx_rooms_federated;
