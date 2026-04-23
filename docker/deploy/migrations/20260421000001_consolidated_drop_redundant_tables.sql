-- ============================================================================
-- Consolidated Migration: Drop Redundant Tables
-- Created: 2026-04-22 (consolidated from 4 migrations dated 2026-04-21 ~ 2026-04-22)
--
-- Merged source files:
--   1. 20260421000001_drop_unused_tables.sql (3 zero-ref tables)
--   2. 20260422000001_drop_redundant_tables_phase_b.sql (4 dead-code tables)
--   3. 20260422000002_drop_redundant_tables_phase_c.sql (9 over-engineered tables)
--   4. 20260422000003_drop_redundant_tables_phase_d.sql (2 retention queue tables)
--
-- Total: 18 tables dropped. All had zero or stub-only code references.
-- See docs/synapse-rust/REDUNDANT_TABLE_DELETION_PLAN.md for analysis.
-- ============================================================================


-- ===== Merged from: 20260421000001_drop_unused_tables.sql =====

-- Drop tables that have no code references and are not part of the Matrix spec.
-- These were over-engineered features that were never wired into the application.
-- Safe: verified zero DML references in src/ for each table.

DROP TABLE IF EXISTS private_messages CASCADE;
DROP TABLE IF EXISTS private_sessions CASCADE;
DROP TABLE IF EXISTS room_children CASCADE;
DROP TABLE IF EXISTS ip_reputation CASCADE;

-- ===== Merged from: 20260422000001_drop_redundant_tables_phase_b.sql =====

-- Migration: Drop redundant tables (Phase B)
-- password_policy: PasswordPolicyService was never instantiated; policy is config-driven
-- key_rotation_history: Redundant with key_rotation_log; routes migrated to key_rotation_log
-- presence_routes: Module system over-engineering; presence routing is built-in
-- password_auth_providers: Module system over-engineering; auth is handled by AuthService/OIDC

DROP TABLE IF EXISTS password_policy CASCADE;
DROP TABLE IF EXISTS key_rotation_history CASCADE;
DROP TABLE IF EXISTS presence_routes CASCADE;
DROP TABLE IF EXISTS password_auth_providers CASCADE;

-- ===== Merged from: 20260422000002_drop_redundant_tables_phase_c.sql =====

-- Migration: Drop redundant tables (Phase C)
-- worker_load_stats: Replaced by tracing::debug! structured logging
-- worker_connections: Replaced by tracing::info! structured logging
-- retention_stats: Replaced by runtime aggregation from retention_cleanup_logs
-- deleted_events_index: Replaced by tracing::info! logging + events.status filtering
-- event_report_history: Replaced by tracing::info! logging, methods return stubs
-- event_report_stats: Replaced by runtime aggregation from event_reports
-- spam_check_results: Replaced by tracing::info! logging, methods return stubs
-- third_party_rule_results: Replaced by tracing::info! logging, methods return stubs
-- rate_limit_callbacks: Module over-engineering, methods return stubs

DROP TABLE IF EXISTS worker_load_stats CASCADE;
DROP TABLE IF EXISTS worker_connections CASCADE;
DROP TABLE IF EXISTS retention_stats CASCADE;
DROP TABLE IF EXISTS deleted_events_index CASCADE;
DROP TABLE IF EXISTS event_report_history CASCADE;
DROP TABLE IF EXISTS event_report_stats CASCADE;
DROP TABLE IF EXISTS spam_check_results CASCADE;
DROP TABLE IF EXISTS third_party_rule_results CASCADE;
DROP TABLE IF EXISTS rate_limit_callbacks CASCADE;

-- ===== Merged from: 20260422000003_drop_redundant_tables_phase_d.sql =====

-- Migration: Drop redundant tables (Phase D - retention queue/logs)
-- retention_cleanup_queue: Replaced by in-memory processing + tracing logging
-- retention_cleanup_logs: Replaced by tracing::info! structured logging
-- The retention service (delete_events_before) still works via direct events table DELETE.

DROP TABLE IF EXISTS retention_cleanup_queue CASCADE;
DROP TABLE IF EXISTS retention_cleanup_logs CASCADE;
