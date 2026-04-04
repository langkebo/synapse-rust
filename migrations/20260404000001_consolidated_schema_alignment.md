# Consolidated Schema Alignment Migration

**Migration ID**: 20260404000001  
**Created**: 2026-04-04  
**Status**: ✅ Tested and verified

## Purpose

Consolidates 10 separate schema alignment migrations into a single unified migration file.

## Consolidated Migrations

This migration replaces the following 10 files (now archived in `migrations/archive/consolidated_20260404/`):

1. `20260330000001_add_thread_replies_and_receipts.sql` (65 lines)
2. `20260330000002_align_thread_schema_and_relations.sql` (13 lines)
3. `20260330000003_align_retention_and_room_summary_schema.sql` (108 lines)
4. `20260330000004_align_space_schema_and_add_space_events.sql` (56 lines)
5. `20260330000005_align_remaining_schema_exceptions.sql` (496 lines)
6. `20260330000006_align_notifications_push_and_misc_exceptions.sql` (115 lines)
7. `20260330000007_align_uploads_and_user_settings_exceptions.sql` (48 lines)
8. `20260330000008_align_background_update_exceptions.sql` (42 lines)
9. `20260330000009_align_beacon_and_call_exceptions.sql` (124 lines)
10. `20260330000013_align_legacy_timestamp_columns.sql` (234 lines)

**Total**: 1,301 lines → 1,383 lines (consolidated with headers)

## Changes Made

### Fixed Issues
- Removed `DO $$ ... $$` wrapper from `20260330000009` that prevented `CREATE INDEX CONCURRENTLY` execution
- All concurrent index creation now happens outside transaction blocks

### Structure
Each original migration is preserved as a distinct "Part" with clear section headers for traceability.

## Testing Results

### Test Environment
- Database: PostgreSQL 16
- Baseline: `00000000_unified_schema_v6.sql` (174 tables)
- Test database: `synapse_consolidation_test`

### Execution Results
- ✅ Baseline applied successfully
- ✅ Consolidated migration executed without errors
- ✅ Final table count: 206 tables (+32 from baseline)
- ✅ Migration record created: `20260404000001`

### Verification
```sql
SELECT version, description FROM schema_migrations WHERE version = '20260404000001';
-- Result: 20260404000001 | Consolidated schema alignment (replaces 20260330000001-20260330000013)
```

## Benefits

1. **Reduced file count**: 10 files → 1 file (-9 files)
2. **Simplified timeline**: Single migration point for all schema alignment work
3. **Easier maintenance**: One file to review/modify instead of 10
4. **Preserved traceability**: Original migration boundaries clearly marked

## Archive Location

Original source files archived to:
```
migrations/archive/consolidated_20260404/
```

Includes:
- 10 original SQL migration files
- 10 corresponding rollback/undo files
- README.md documenting the consolidation

## Deployment Status

- ✅ Consolidated migration created and tested
- ✅ Original files archived
- ✅ CI workflows updated to use consolidated migration
- ✅ Rollback support implemented via `.undo.sql`

## Rollback Plan

If issues arise:
1. Restore original 10 files from backup
2. Remove consolidated file
3. Re-run original migration sequence

Original files remain in backup directory for safety.
