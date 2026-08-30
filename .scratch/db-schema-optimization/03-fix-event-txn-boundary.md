# DB-03: 修复事件写入事务边界 + 消除 Txn 去重竞态

**What to build:** Refactor `EventWriter::create_event` to require an explicit transaction (remove `Option<&mut Transaction>`), and fix the `record_event_txn` dedup mechanism to use `INSERT ... ON CONFLICT DO NOTHING` with `RETURNING` instead of a SELECT-then-INSERT race window.

**Blocked by:** None (can start immediately)

## Steps

### 3a. Read the current event writer implementation

Examine:
- `synapse-storage/src/event/writer.rs` — the `create_event` signature and implementation
- `synapse-storage/src/event/txn_dedup.rs` — the dedup logic
- All callers of `create_event` across the codebase:
```bash
grep -rn "create_event\|EventWriter" synapse-storage/src/ --include="*.rs"
```

### 3b. Audit all call sites

For each call site that currently passes `tx: None`, refactor to use a transaction:

```rust
// Before
let event = writer.create_event(params, None).await?;

// After
let mut tx = pool.begin().await?;
let event = writer.create_event(params, Some(&mut tx)).await?;
tx.commit().await?;
```

Ensure every call site wraps the write in a transaction. Document any that legitimately need auto-commit (there shouldn't be any for event writes).

### 3c. Fix the dedup race condition

In `txn_dedup.rs`, replace:

```rust
// BEFORE (race window between SELECT and INSERT):
let exists = sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM room_event_txn_dedup WHERE user_id = $1 AND room_id = $2 AND txn_id = $3)"
).bind(user_id).bind(room_id).bind(txn_id)
.fetch_one(&mut *tx).await?;

if exists {
    return Ok(event_id); // already written, skip
}
sqlx::query("INSERT INTO room_event_txn_dedup ...").execute(&mut *tx).await?;
```

With:

```rust
// AFTER (atomic, no race window):
let result = sqlx::query(
    r"INSERT INTO room_event_txn_dedup (user_id, room_id, txn_id, event_id)
      VALUES ($1, $2, $3, $4)
      ON CONFLICT (user_id, room_id, txn_id) DO NOTHING
      RETURNING event_id"
)
.bind(user_id).bind(room_id).bind(txn_id).bind(event_id)
.fetch_optional(&mut *tx)
.await?;

// If a row was returned, this txn_id was already committed — return early
if let Some(existing) = result {
    return Ok(existing.event_id);
}
```

Verify that `room_event_txn_dedup` has a `UNIQUE (user_id, room_id, txn_id)` constraint. If not, add it in a migration.

### 3d. Propagate transaction requirement

Change the signature:

```rust
// Before
async fn create_event(
    &self, params: CreateEventParams,
    tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
) -> Result<RoomEvent, sqlx::Error>

// After
async fn create_event(
    &self, params: CreateEventParams,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<RoomEvent, sqlx::Error>
```

Update the trait signature if there's a `EventWriter` trait.

### 3e. Run tests

```bash
cargo test --package synapse-storage --lib event -- --nocapture
```

### 3f. Commit

```bash
git add synapse-storage/src/event/
git commit -m "fix(event): require explicit transaction + fix txn dedup race condition"
```
