# DB-02: 扩充 Schema 健康检查覆盖范围

**What to build:** Extend `synapse-storage/src/schema_health_check.rs` to cover all tables defined in `00000000_unified_schema_v10.sql` (currently CORE_TABLES has ~29 entries but there are ~90+ tables). Also refactor the static list to a dynamic scan that verifies the baseline against `information_schema.tables`, making future table additions automatically covered.

**Blocked by:** None (can start immediately)

## Steps

### 3a. Inventory all tables from the baseline file

Extract the full list of `CREATE TABLE` table names from the baseline:

```bash
grep "^CREATE TABLE" migrations/00000000_unified_schema_v10.sql \
  | grep -oP '(?<=CREATE TABLE )\w+' \
  | sort -u
```

Cross-reference with tables created by delta migrations (the files after `00000000_*.sql`):

```bash
grep "^CREATE TABLE" migrations/*.sql \
  | grep -oP '(?<=CREATE TABLE )\w+' \
  | sort -u
```

This gives the authoritative full table list.

### 3b. Refactor CORE_TABLES to use a source-of-truth

Option A — Static update (simpler): Add all missing table names to the `CORE_TABLES` constant. Group them by category in comments:

```rust
const CORE_TABLES: &[&str] = &[
    // Core auth & users
    "users", "refresh_tokens", "devices", "device_lists_outbound_poks",
    // ...
    // Sliding sync
    "sliding_sync_sync_tables", "sliding_sync_rooms", "sliding_sync_memberships",
    // ...
];
```

Option B — Dynamic scan (better): Query `information_schema.tables WHERE table_schema = 'public'` at runtime and diff against the baseline migration record. Propose Option B.

### 3c. Add migration completeness check

In `run_schema_health_check()`, add a new check that verifies all baseline migrations have been applied by querying `_sqlx_migrations`. If any migration is missing, the health check should fail with a list of missing version numbers.

### 3d. Test

Write a test in `schema_health_check.rs` that:
1. Reads the actual table count from `information_schema.tables`
2. Compares against the expected count from CORE_TABLES
3. Fails if they diverge by more than a configurable threshold (to allow for temp tables)

Run: `cargo test --package synapse-storage --lib schema_health_check -- --nocapture`

### 3e. Commit

```bash
git add synapse-storage/src/schema_health_check.rs
git commit -m "fix(schema): expand CORE_TABLES to cover all baseline tables + add migration completeness check"
```
