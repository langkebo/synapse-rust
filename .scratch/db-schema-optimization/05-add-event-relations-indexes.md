# DB-05: 为 event_relations 添加缺失索引

**What to build:** Add GIN index on `event_relations.content` (JSONB) and a covering index on `event_relations.relates_to_event_id` to fix single-column lookup performance. This is a simple index-only migration with zero risk of breaking existing queries.

**Blocked by:** None (can start immediately)

## Steps

### 3a. Verify thread_replies.content GIN index pattern

Look at the existing GIN index on `thread_replies` for reference:

```bash
grep -n "GIN.*thread_replies\|GIN.*content" migrations/00000000_unified_schema_v10.sql
```

### 3b. Add the new indexes

Create migration `migrations/YYYYMMDDHHMMSS_add_event_relations_indexes.sql`:

```sql
-- GIN index on event_relations.content for JSONB content searches
CREATE INDEX IF NOT EXISTS idx_event_relations_content_gin
    ON event_relations USING GIN (content jsonb_path_ops);

-- Covering index for single-column relates_to_event_id lookups
-- (e.g. "get all thread replies for this event" without requiring room_id)
CREATE INDEX IF NOT EXISTS idx_event_relations_relates_to
    ON event_relations(relates_to_event_id);
```

### 3c. Verify existing queries benefit from these indexes

Check the event relations query paths:

```bash
grep -rn "event_relations" synapse-storage/src/ --include="*.rs" -A 3
```

Confirm the `relates_to_event_id` column is used in single-column form (without room_id) in any query path.

### 3d. Verify

```bash
cargo sqlx migrate run
psql -c "\\d event_relations" | grep idx_event_relations
```

### 3e. Commit

```bash
git add migrations/
git commit -m "perf(db): add GIN and covering indexes on event_relations"
```
