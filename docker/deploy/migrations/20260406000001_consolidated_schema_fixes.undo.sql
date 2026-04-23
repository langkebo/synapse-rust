-- Undo: Consolidated Schema Fixes (reverse order)

-- ===== From: 20260406000006_align_media_quota_schema_contract.undo.sql =====
-- ============================================================================
-- Rollback: align_media_quota_schema_contract
-- Created: 2026-04-06
-- Description: Removes media quota schema contract alignment artifacts.
-- ============================================================================

SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_media_quota_alerts_user;
DROP INDEX IF EXISTS idx_media_usage_log_timestamp;
DROP INDEX IF EXISTS idx_media_usage_log_user;

DROP TABLE IF EXISTS media_quota_alerts;
DROP TABLE IF EXISTS media_usage_log;
DROP TABLE IF EXISTS server_media_quota;

ALTER TABLE IF EXISTS user_media_quota
    DROP COLUMN IF EXISTS quota_config_id,
    DROP COLUMN IF EXISTS custom_max_storage_bytes,
    DROP COLUMN IF EXISTS custom_max_file_size_bytes,
    DROP COLUMN IF EXISTS custom_max_files_count,
    DROP COLUMN IF EXISTS current_storage_bytes,
    DROP COLUMN IF EXISTS current_files_count;

ALTER TABLE IF EXISTS media_quota_config
    DROP COLUMN IF EXISTS name,
    DROP COLUMN IF EXISTS description,
    DROP COLUMN IF EXISTS max_storage_bytes,
    DROP COLUMN IF EXISTS max_file_size_bytes,
    DROP COLUMN IF EXISTS max_files_count,
    DROP COLUMN IF EXISTS allowed_mime_types,
    DROP COLUMN IF EXISTS blocked_mime_types,
    DROP COLUMN IF EXISTS is_default;

-- ===== From: 20260406000005_align_presence_schema_contract.undo.sql =====
-- ============================================================================
-- Rollback: align_presence_schema_contract
-- Created: 2026-04-06
-- Description: Restores nullable/default behavior for legacy presence columns
-- if a rollback to the pre-contract shape is required.
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence DROP NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN presence DROP DEFAULT;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts DROP NOT NULL;

ALTER TABLE IF EXISTS presence
    ALTER COLUMN last_active_ts DROP DEFAULT;

DROP INDEX IF EXISTS idx_presence_user_status;

-- ===== From: 20260406000004_cleanup_schema_contract_room_orphans.undo.sql =====
-- ============================================================================
-- Rollback: cleanup_schema_contract_room_orphans
-- Created: 2026-04-06
-- Description: This cleanup migration is irreversible because it deletes
-- orphan rows from derived tables.
-- ============================================================================

SET TIME ZONE 'UTC';

-- Irreversible: deleted orphan rows cannot be reconstructed from this script.

-- ===== From: 20260406000003_restore_public_schema_contract_foreign_keys.undo.sql =====
-- ============================================================================
-- Rollback: restore_public_schema_contract_foreign_keys
-- Created: 2026-04-06
-- Description: Drops the public schema foreign keys restored by
-- 20260406000003.
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS deleted_events_index
    DROP CONSTRAINT IF EXISTS fk_deleted_events_index_room;
ALTER TABLE IF EXISTS retention_stats
    DROP CONSTRAINT IF EXISTS fk_retention_stats_room;
ALTER TABLE IF EXISTS retention_cleanup_logs
    DROP CONSTRAINT IF EXISTS fk_retention_cleanup_logs_room;
ALTER TABLE IF EXISTS retention_cleanup_queue
    DROP CONSTRAINT IF EXISTS fk_retention_cleanup_queue_room;
ALTER TABLE IF EXISTS room_children
    DROP CONSTRAINT IF EXISTS fk_room_children_child;
ALTER TABLE IF EXISTS room_children
    DROP CONSTRAINT IF EXISTS fk_room_children_parent;
ALTER TABLE IF EXISTS room_summary_update_queue
    DROP CONSTRAINT IF EXISTS fk_room_summary_update_queue_room;
ALTER TABLE IF EXISTS room_summary_stats
    DROP CONSTRAINT IF EXISTS fk_room_summary_stats_room;
ALTER TABLE IF EXISTS room_summary_state
    DROP CONSTRAINT IF EXISTS fk_room_summary_state_room;

-- ===== From: 20260406000002_restore_schema_contract_foreign_keys.undo.sql =====
-- ============================================================================
-- Rollback: restore_schema_contract_foreign_keys
-- Created: 2026-04-06
-- Description: Drops the room summary and retention foreign keys restored by
-- 20260406000002.
-- ============================================================================

SET TIME ZONE 'UTC';

ALTER TABLE IF EXISTS deleted_events_index
    DROP CONSTRAINT IF EXISTS fk_deleted_events_index_room;
ALTER TABLE IF EXISTS retention_stats
    DROP CONSTRAINT IF EXISTS fk_retention_stats_room;
ALTER TABLE IF EXISTS retention_cleanup_logs
    DROP CONSTRAINT IF EXISTS fk_retention_cleanup_logs_room;
ALTER TABLE IF EXISTS retention_cleanup_queue
    DROP CONSTRAINT IF EXISTS fk_retention_cleanup_queue_room;
ALTER TABLE IF EXISTS room_children
    DROP CONSTRAINT IF EXISTS fk_room_children_child;
ALTER TABLE IF EXISTS room_children
    DROP CONSTRAINT IF EXISTS fk_room_children_parent;
ALTER TABLE IF EXISTS room_summary_update_queue
    DROP CONSTRAINT IF EXISTS fk_room_summary_update_queue_room;
ALTER TABLE IF EXISTS room_summary_stats
    DROP CONSTRAINT IF EXISTS fk_room_summary_stats_room;
ALTER TABLE IF EXISTS room_summary_state
    DROP CONSTRAINT IF EXISTS fk_room_summary_state_room;

-- ===== From: 20260406000001_restore_verification_requests_pending_index.undo.sql =====
-- ============================================================================
-- Rollback: restore_verification_requests_pending_index
-- Created: 2026-04-06
-- Description: Drops the verification_requests pending lookup index restored by
-- 20260406000001.
-- ============================================================================

SET TIME ZONE 'UTC';

DROP INDEX IF EXISTS idx_verification_requests_to_user_state;

-- ===== From: 20260405000002_fix_push_rules_unique_constraint_v2.undo.sql =====
ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_rule UNIQUE (user_id, scope, rule_id);


-- ===== From: 20260405000001_fix_push_rules_unique_constraint.undo.sql =====
ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_rule UNIQUE (user_id, scope, rule_id);

