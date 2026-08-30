# DB-01: 清理 Schema 基线中的重复索引并统一迁移跟踪表

**What to build:** Deduplicate index definitions in `00000000_unified_schema_v10.sql`. The redundant `schema_migrations` table is **NOT redundant** — it is an intentional project design choice used by the schema validator. See corrected P0-5 in the diagnostic report.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

## Steps

### 3a. Deduplicate indexes

In `migrations/00000000_unified_schema_v10.sql`, search for duplicate `CREATE INDEX` / `CREATE UNIQUE INDEX` statements for the same column set. Known duplicates to remove:

| Duplicate index | Keep index | Reason |
|---|---|---|
| `idx_memberships_user_room` (line ~3522) | `idx_room_memberships_room_user` (line ~3996) | Same column set, keep canonical name |
| `idx_events_friend_room` (lines ~4006-4009) | first occurrence | Duplicate definition |
| `idx_access_tokens_token_hash` (non-unique, line ~4027) | `uq_access_tokens_token_hash` (unique, line ~4024) | Non-unique is redundant with unique |

Run this query to find all duplicates in the baseline file:

```bash
grep -n "^CREATE INDEX\|^CREATE UNIQUE INDEX" migrations/00000000_unified_schema_v10.sql | sort -k3 | uniq -D -f2
```

Remove each duplicate `CREATE INDEX` line. Do NOT drop existing indexes in the database — the `CREATE INDEX IF NOT EXISTS` guards already handle that. Only remove the redundant DDL lines from the baseline file.

### 3b. Remove schema_migrations table

The sqlx migration runner (`sqlx migrate run`) creates and uses `_sqlx_migrations` automatically. The project's `schema_migrations` table (defined in `00000000_unified_schema_v10.sql`) is redundant.

Create a new migration file `migrations/YYYYMMDDHHMMSS_drop_schema_migrations.sql`:

```sql
-- Drop the redundant schema_migrations table
-- The sqlx standard _sqlx_migrations table is used instead
DROP TABLE IF EXISTS schema_migrations;
```

Verify the table exists in the baseline before adding the drop. Run `cargo sqlx migrate add drop_schema_migrations` to create the migration properly, then write the SQL.

### 3c. Verify

- `cargo sqlx migrate run` — succeeds with no errors
- `cargo check` — clean
- `grep -c "CREATE INDEX" migrations/00000000_unified_schema_v10.sql` — count decreased by number of duplicates removed
- `SELECT * FROM _sqlx_migrations;` — shows all migrations as applied

### 3d. Commit

```bash
git add migrations/
git commit -m "refactor(db): dedupe indexes + remove redundant schema_migrations table"
```
