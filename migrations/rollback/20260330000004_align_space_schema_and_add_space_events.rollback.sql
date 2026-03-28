-- Rollback script for 20260330000004_align_space_schema_and_add_space_events.sql
-- This script reverses the schema changes made by the forward migration
-- Note: This rollback is for emergency use only. Data loss may occur.

-- Drop new tables (data will be lost)
DROP TABLE IF EXISTS space_events;
DROP TABLE IF EXISTS space_statistics;
DROP TABLE IF EXISTS space_summaries;
DROP TABLE IF EXISTS space_members;

-- Drop indexes
DROP INDEX IF EXISTS idx_spaces_parent;
DROP INDEX IF EXISTS idx_space_summary_space;
DROP INDEX IF EXISTS idx_space_statistics_member_count;
