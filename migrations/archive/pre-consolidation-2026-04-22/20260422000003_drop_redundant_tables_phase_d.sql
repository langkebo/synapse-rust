-- Migration: Drop redundant tables (Phase D - retention queue/logs)
-- retention_cleanup_queue: Replaced by in-memory processing + tracing logging
-- retention_cleanup_logs: Replaced by tracing::info! structured logging
-- The retention service (delete_events_before) still works via direct events table DELETE.

DROP TABLE IF EXISTS retention_cleanup_queue CASCADE;
DROP TABLE IF EXISTS retention_cleanup_logs CASCADE;
