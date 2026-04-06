-- ============================================================================
-- Rollback: cleanup_schema_contract_room_orphans
-- Created: 2026-04-06
-- Description: This cleanup migration is irreversible because it deletes
-- orphan rows from derived tables.
-- ============================================================================

SET TIME ZONE 'UTC';

-- Irreversible: deleted orphan rows cannot be reconstructed from this script.
