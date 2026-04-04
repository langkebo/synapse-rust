# Archived Schema Alignment Migrations

**Archive Date**: 2026-04-04  
**Reason**: Consolidated into single migration file

## Archived Files

These 10 migration files have been consolidated into:
`migrations/20260404000001_consolidated_schema_alignment.sql`

### Original Files (in execution order)
1. 20260330000001_add_thread_replies_and_receipts.sql
2. 20260330000002_align_thread_schema_and_relations.sql
3. 20260330000003_align_retention_and_room_summary_schema.sql
4. 20260330000004_align_space_schema_and_add_space_events.sql
5. 20260330000005_align_remaining_schema_exceptions.sql
6. 20260330000006_align_notifications_push_and_misc_exceptions.sql
7. 20260330000007_align_uploads_and_user_settings_exceptions.sql
8. 20260330000008_align_background_update_exceptions.sql
9. 20260330000009_align_beacon_and_call_exceptions.sql
10. 20260330000013_align_legacy_timestamp_columns.sql

## Why Archived?

- Reduces migration file count from 31 to 22 (-9 files)
- Simplifies migration timeline
- Easier maintenance and review
- All functionality preserved in consolidated file

## Restoration

If needed, these files can be restored from this archive directory.

## See Also

- `migrations/20260404000001_consolidated_schema_alignment.md` - Full consolidation documentation
- `migrations/.backup_consolidation_20260404_120308/` - Additional backup location
