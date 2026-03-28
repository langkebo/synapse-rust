-- Rollback script for 20260330000002_align_thread_schema_and_relations.sql
-- This script reverses the schema changes made by the forward migration
-- Note: This rollback is for emergency use only. Data loss may occur.

-- Drop new table (data will be lost)
DROP TABLE IF EXISTS thread_relations;

-- Drop indexes
DROP INDEX IF EXISTS idx_thread_roots_room_thread_unique;
DROP INDEX IF EXISTS idx_thread_roots_room_last_reply_created;
DROP INDEX IF EXISTS idx_thread_replies_room_thread_event;
