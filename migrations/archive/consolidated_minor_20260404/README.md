# Consolidated Minor Features Archive

This directory contains the original source migrations that were consolidated into `20260404000002_consolidated_minor_features.sql`.

## Archived Files

1. **20260328000002_add_federation_cache.sql** (10 lines)
   - Created federation_cache table
   - Added 2 concurrent indexes

2. **20260330000010_add_audit_events.sql** (empty after duplicate removal)
   - Originally defined audit_events table
   - Duplicate definition removed; table now only in unified baseline

3. **20260330000011_add_feature_flags.sql** (32 lines)
   - Created feature_flags table
   - Created feature_flag_targets table
   - Added 3 concurrent indexes

## Consolidation Details

- **Consolidation Date**: 2026-04-04
- **Target File**: `migrations/20260404000002_consolidated_minor_features.sql`
- **Reason**: Reduce migration file count, improve maintainability
- **Total Lines Consolidated**: 62 lines (excluding empty audit_events)

## Rollback

The consolidated rollback is in:
- `migrations/20260404000002_consolidated_minor_features.undo.sql`

Original rollback files archived:
- `20260330000010_add_audit_events.undo.sql`
- `20260330000011_add_feature_flags.undo.sql`
- `migrations/rollback/20260328000002_add_federation_cache.rollback.sql` (converted to no-op)

## Migration Path

### Fresh Install
Apply unified baseline (`00000000_unified_schema_v6.sql`) which now includes all three features.

### Upgrade Path
Apply `20260404000002_consolidated_minor_features.sql` which uses `IF NOT EXISTS` for idempotency.

## Notes

- Original files in main migrations/ directory converted to no-op placeholders
- CI workflow updated to reference consolidated file
- All tests validated with consolidated migration
