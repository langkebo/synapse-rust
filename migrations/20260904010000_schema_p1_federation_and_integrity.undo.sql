-- Migration undo: 20260904010000_schema_p1_federation_and_integrity.sql

-- P1-1: device_signatures 索引（可立即删除）
DROP INDEX CONCURRENTLY IF EXISTS idx_device_signatures_user_device;
DROP INDEX CONCURRENTLY IF EXISTS idx_device_signatures_target;

-- P1-2: room_memberships CHECK
ALTER TABLE room_memberships DROP CONSTRAINT IF EXISTS ck_room_memberships_valid;

-- P1-3: event_edges FK
ALTER TABLE event_edges DROP CONSTRAINT IF EXISTS fk_event_edges_prev;
DROP INDEX CONCURRENTLY IF EXISTS idx_event_edges_prev_room;

-- P1-4: events.redacted_by FK
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_redacted_by;
