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
