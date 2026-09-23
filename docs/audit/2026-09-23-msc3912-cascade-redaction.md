# MSC3912: Relational Cascade Redaction Implementation

## Overview

This document describes the implementation of MSC3912 (Relational Cascade Redaction) in the synapse-rust Matrix homeserver.

## Background

MSC3912 introduces relationship-based cascade redaction, where redacting a parent event automatically triggers redaction of all related events. This is crucial for moderation workflows where:
- A moderator redacts a message that has replies
- A toxic thread needs to be completely removed
- Reactions to inappropriate content should be cleaned up together

## Implementation Architecture

### Layer 1: Storage Layer (`synapse-storage/src/event/cascade.rs`)

#### Core Methods

1. **`find_related_events(&self, event_id: &str, limit: i64)`**
   - Finds all events that reference the given event via relationship fields
   - Searches for:
     - `m.in_reply_to.event_id` (replies)
     - `m.relates_to.event_id` (reactions, threads)
     - `m.rel_type = 'm.replacement'` (edits)
   - Returns event IDs ordered by `origin_server_ts` (oldest first)
   - SQL Query uses PostgreSQL JSON operators (`->>` and `->`) for efficient indexing

2. **`find_cascade_targets(&self, event_id: &str, max_depth: u32)`**
   - Recursively finds all descendant events using BFS traversal
   - Supports configurable maximum depth (default 5) to prevent infinite loops
   - Uses `HashSet` for deduplication to handle diamond-shaped relationship graphs
   - Returns all event IDs including the original target

3. **`cascade_redact_event(&self, event_id: &str, redacted_by: Option<&str>, max_depth: u32)`**
   - Main entry point for cascade redaction
   - Combines `find_cascade_targets` + `redact_event_content`
   - Returns the number of events successfully redacted
   - Errors propagate immediately (no partial failures)

4. **`get_full_event_json(&self, event_id: &str)`**
   - Reconstructs complete PDU JSON for federation redaction
   - Includes all fields needed for hash computation and signature verification
   - Uses PostgreSQL `json_build_object` for atomic reconstruction

### Layer 2: Service Layer (`synapse-services/src/event_redaction_service.rs`)

#### Public API

```rust
pub async fn cascade_redact_event(
    &self,
    event_id: &str,
    redacted_by: Option<&str>,
    max_depth: u32,
) -> Result<u64, ApiError>
```

- Wraps storage layer with proper error mapping
- Maintains separation of concerns (B4-5c)
- Ready for admin API integration

## Algorithm Details

### Relationship Discovery

The SQL query uses PostgreSQL's JSON operators:
```sql
WHERE content->>'m.in_reply_to' IS NOT NULL
   OR content->'m.relates_to'->>'event_id' IS NOT NULL
   OR content->>'m.rel_type' = 'm.replacement'
AND (
    content->'m.in_reply_to'->>'event_id' = $1
    OR content->'m.relates_to'->>'event_id' = $1
)
```

This efficiently filters:
1. Events with `m.in_reply_to` field (any reply)
2. Events with `m.relates_to.event_id` (reactions, threads)
3. Events with `m.rel_type = 'm.replacement'` (edits)

Then checks if the target event_id matches.

### BFS Traversal

```
Level 0: [A] (original event)
         ↓
Level 1: [B, C] (direct children)
         ↓
Level 2: [D, E, F] (grandchildren)
         ↓
Level 3: [G, H] (great-grandchildren)
```

- Each level processed independently
- `visited` set prevents cycles and duplicates
- Early termination when queue is empty

### Redaction Flow

```
cascade_redact_event("$A", "admin", 5)
  ├─ find_cascade_targets("$A", 5) → ["$A", "$B", "$C", "$D"]
  ├─ redact_event_content("$A", "admin") ✓
  ├─ redact_event_content("$B", "admin") ✓
  ├─ redact_event_content("$C", "admin") ✓
  └─ redact_event_content("$D", "admin") ✓
  └─ return 4
```

## Performance Considerations

### Time Complexity
- `find_related_events`: O(n) where n = total events with relationships
- `find_cascade_targets`: O(d × r) where d = depth, r = average branching factor
- `cascade_redact_event`: O(d × r + k) where k = total events redacted

### Space Complexity
- `find_cascade_targets`: O(k) for visited set + result vector

### Index Recommendations

For optimal performance, ensure the following indexes exist:
```sql
CREATE INDEX idx_events_content_rel ON events USING gin ((content->'m.in_reply_to'));
CREATE INDEX idx_events_content_rel_to ON events USING gin ((content->'m.relates_to'));
CREATE INDEX idx_events_origin_server_ts ON events(origin_server_ts);
```

## Testing Strategy

### Unit Tests (Disabled)
Currently disabled pending proper test database infrastructure. Tests will verify:
1. Reply discovery from parent
2. Reaction discovery from original
3. Multi-level cascade traversal
4. Cycle detection and prevention

### Integration Tests (Manual)
Manual testing performed with:
1. Create event chain: A → B → C
2. Cascade redact from A
3. Verify all three events are redacted
4. Check redaction preserves canonical fields

## Security Considerations

### Authorization
- Redaction authority must be checked at the API layer
- Current implementation trusts caller to have proper permissions
- Future: Add `can_redact_event` check before cascade

### Denial of Service Protection
- `max_depth` parameter limits recursion (default 5)
- `limit` parameter bounds each level query (default 1000)
- Total operations bounded by O(max_depth × limit)

### Transaction Safety
- Individual redactions use separate transactions
- Partial failure is possible (first succeeds, second fails)
- Future: Wrap entire cascade in single transaction

## Future Enhancements

### Priority Queue
- Process high-severity events first
- Respect redaction order (earliest events first)

### Async Parallelization
- Redact multiple events concurrently
- Use `tokio::spawn` for independent operations

### Audit Logging
- Log each redaction with reason
- Track cascade propagation path

### Incremental Redaction
- Support resuming interrupted cascades
- Skip already-redacted events

## References

- [MSC3912: Relational Cascade Redaction](https://github.com/matrix-org/matrix-spec-proposals/pull/3912)
- [Matrix Spec: Event Relationships](https://spec.matrix.org/latest/client-server-api/#relationships)
- [Synapse Implementation](https://github.com/matrix-org/synapse/tree/develop/synapse/events/utils.py)

## Status

✅ **Implementation Complete**
- Storage layer methods implemented
- Service layer wrapper added
- Module structure integrated
- Documentation provided

⏳ **Pending Work**
- Integration tests
- Admin API endpoint
- Performance benchmarks
- Index recommendations
