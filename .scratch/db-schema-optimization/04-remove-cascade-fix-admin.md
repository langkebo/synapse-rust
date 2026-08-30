# DB-04: 修复默认 admin 账号硬编码 + 记录 events CASCADE 风险

**What to build:** Two-part fix split by risk:

**Part A (this ticket)**: Remove the hardcoded default admin account from `00000000_unified_schema_v10.sql` and move it to a dedicated `scripts/create-default-admin.sql` post-deploy script. Low risk, no FK changes.

**Part B (deferred to DB-04-b)**: The `events` `ON DELETE CASCADE` FK fix is **NOT** in this ticket. Investigation shows that `RoomStorage::delete_room` (`synapse-storage/src/room/mod.rs:778`) **relies entirely on CASCADE** to clean up events, room_memberships, and room_aliases. Removing the FK without rewriting delete_room would break room deletion entirely. The CASCADE risk (P0-3) requires both a migration AND a Rust refactor of delete_room to manually batch-delete events first. That's a larger change, scoped as DB-04-b in a follow-up ticket.

**Blocked by:** None (can start immediately)

## Steps

### Part A: Move default admin to a deploy script

The hardcoded admin INSERT is in `00000000_unified_schema_v10.sql` near the end. Find the INSERT into `users` with username = 'admin' and remove it. Then create `scripts/create-default-admin.sql`:

```sql
-- Default admin account bootstrap (post-deploy)
-- RUN ONLY ON FRESH DATABASE INITIALIZATION
-- Delete this account before exposing server to production users.

INSERT INTO users (user_id, username, password_hash, is_admin, must_change_password, created_ts)
SELECT
    '@admin:localhost',
    'admin',
    '$argon2id$v=19$m=65536,t=3,p=1$VGVzdFNhbHRGb3JBZG1pbg$K7G8H5J3M2N9P4Q6R8S0T2U4V6W8X0Y2Z4A6B8C0D2E4F6G8H0J2K4L6M8N0P2Q4',
    TRUE,
    TRUE,
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
WHERE NOT EXISTS (SELECT 1 FROM users WHERE username = 'admin');
```

Update `docker/db_migrate.sh` (or document in README) to call this script as an optional step on first install only.

### Part B: Defer the events CASCADE change

Add a `// TODO: DB-04-b` comment in `synapse-storage/src/room/mod.rs` near `delete_room`:

```rust
// TODO: DB-04-b — Replace CASCADE FK with manual batched cleanup.
// See artifacts/数据库架构诊断报告-2026-08-30.md §P0-3.
pub async fn delete_room(&self, room_id: &str) -> Result<(), sqlx::Error> { ... }
```

Do NOT actually change the FK or delete_room yet — that's a separate, larger refactor.

### Verify

- `python3 scripts/check_baseline_consolidation.py` — pass
- `cargo check --all` — clean
- `grep -c "admin" migrations/00000000_unified_schema_v10.sql` — decreased (admin INSERT removed)
- `grep -c "TODO: DB-04-b" synapse-storage/src/room/mod.rs` — exactly 1

### Commit

```bash
git add migrations/00000000_unified_schema_v10.sql scripts/create-default-admin.sql synapse-storage/src/room/mod.rs
git commit -m "fix(schema): move default admin to deploy script + flag DB-04-b deferred

DB-04 Part A:
- Removed hardcoded admin INSERT from baseline (00000000_unified_schema_v10.sql)
- Added scripts/create-default-admin.sql for opt-in bootstrap
- Added TODO comment in delete_room flagging future DB-04-b (CASCADE FK refactor)
- baseline_consolidation: passes
- cargo check --all: clean"
```
