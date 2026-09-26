# v12 PDU Graph Fields Integration (O-1 Phase 1)

## Overview

This document records the integration of `depth` calculation and `auth_events` construction into the message creation pipeline for v12+ rooms.

## Architecture Review

### PduGraphFields Structure

```rust
pub struct PduGraphFields {
    pub depth: Option<i64>,           // max(prev_event_depths) + 1
    pub prev_events: Option<Vec<String>>, // Forward extremities
    pub auth_events: Option<Vec<String>>, // Authorization events
}
```

### Existing Infrastructure (Already Implemented)

| Module | File | Purpose |
|--------|------|---------|
| `synapse-storage/src/event/models.rs` | `PduGraphFields` | Struct definition for v12 graph fields |
| `synapse-storage/src/event/depth.rs` | `calculate_event_depth()` | Depth calculation from prev_events |
| `synapse-storage/src/event/dag.rs` | `get_forward_extremities_in_room()` | Get room extremities |
| `synapse-services/src/room/auth.rs` | `AuthEventBuilder` | Auth events construction |
| `synapse-storage/src/event/create.rs` | `create_event_with_pdu()` | Low-level write path with PDU fields |

### Integration Points

#### 1. EventReader Trait Extension

Added to `synapse-storage/src/event/reader.rs`:

```rust
// Trait declaration
async fn calculate_event_depth(&self, room_id: &str, prev_events: &[String]) -> Result<i64, sqlx::Error>;
async fn get_forward_extremities_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error>;

// EventStorage impl delegation
async fn calculate_event_depth(&self, room_id: &str, prev_events: &[String]) -> Result<i64, sqlx::Error> {
    self.calculate_event_depth(room_id, prev_events).await
}

async fn get_forward_extremities_in_room(&self, room_id: &str, limit: i64) -> Result<Vec<String>, sqlx::Error> {
    self.get_forward_extremities_in_room(room_id, limit).await
}
```

#### 2. EventWriter Trait Extension

Added to `synapse-storage/src/event/writer.rs`:

```rust
async fn create_event_with_pdu(
    &self,
    params: CreateEventParams,
    pdu_graph: PduGraphFields,
    tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
) -> Result<RoomEvent, sqlx::Error>;
```

Delegation added to both:
- `GraphMetadataWriter` (delegates to inner writer)
- `NotifyingEventWriter` (delegates to inner writer)

#### 3. Message Creation Integration

Modified `synapse-services/src/room/messaging/events.rs`:

```rust
pub async fn create_event(
    &self,
    mut params: CreateEventParams,
    tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
) -> ApiResult<synapse_storage::RoomEvent> {
    // ... redaction handling ...
    
    // v12+ path: complete PDU graph fields
    if let Some(room_version_str) = room_version {
        if room_version_str.as_str() >= "12" {
            let state_events = self.event_reader.get_state_events(&room_id).await?;
            let auth_builder = auth::AuthEventBuilder::new(state_events);
            
            let prev_events = self.event_reader.get_forward_extremities_in_room(&room_id, 10).await?;
            let depth = self.event_reader.calculate_event_depth(&room_id, &prev_events).await?;
            let auth_events = auth_builder.build_auth_events(&event_type, state_key.as_deref(), &params.user_id);
            
            return self.event_writer.create_event_with_pdu(
                params,
                synapse_storage::event::PduGraphFields {
                    depth: Some(depth),
                    prev_events: Some(prev_events),
                    auth_events: Some(auth_events),
                },
                tx,
            ).await;
        }
    }
    
    // v11 or earlier: legacy path
    let event = self.event_writer.create_event(params, tx).await?;
    // ...
}
```

## v12 PDU Requirements (Matrix Spec)

Per MSC4311 and the Matrix v12 spec, every event in a v12 room must include:

1. **`depth`** — `max(prev_event_depths) + 1`
2. **`prev_events`** — Forward extremities (latest events)
3. **`auth_events`** — Authorization chain containing:
   - `m.room.create` (always required)
   - `m.room.power_levels` (if present in state)
   - `m.room.member` for the creator (always required)
   - `m.room.history_visibility` (if present in state)

## Flow Diagram

```
Client sends event
    ↓
send_message()
    ↓
create_event()  ← v12+ check
    ├── get_state_events()
    ├── get_forward_extremities_in_room()
    ├── calculate_event_depth()
    ├── AuthEventBuilder::build_auth_events()
    └── create_event_with_pdu()
        └── INSERT INTO events (with depth, prev_events, auth_events)
```

## Files Modified

| File | Change |
|------|--------|
| `synapse-storage/src/event/reader.rs` | Added `calculate_event_depth` and `get_forward_extremities_in_room` to `EventReader` trait + impl |
| `synapse-storage/src/event/writer.rs` | Added `create_event_with_pdu` to `EventWriter` trait + impl |
| `synapse-services/src/room/mod.rs` | Added `pub mod auth` |
| `synapse-services/src/room/auth.rs` | New: `AuthEventBuilder` struct and tests |
| `synapse-services/src/room/messaging/events.rs` | Integrated v12+ path in `create_event()` |
| `synapse-services/src/notifying_event_writer.rs` | Implemented `create_event_with_pdu` |
| `synapse-services/src/graph_metadata.rs` | Implemented `create_event_with_pdu` |

## Compilation Status

- ✅ `cargo check --workspace` — PASS
- ✅ All `synapse-storage` tests — PASS (including depth.rs db_tests)

## Known Limitations

1. **Signatures and Hashes**: v12 also requires ED25519-only signatures and content hashes. These are handled by the existing `sign_and_broadcast_event()` path, but the actual signature computation is not yet implemented.
2. **Room Version Check**: Currently uses string comparison (`room_version_str.as_str() >= "12"`). Should use proper semver comparison in production.
3. **Transaction Safety**: The v12+ path does not validate ED25519-only auth rules before committing.

## Next Steps

1. Implement ED25519-only signature verification for v12 events
2. Add content hash computation (SHA-256) for v12 PDUs
3. Write integration tests for v12 event creation
4. Add Prometheus metrics for v12 PDU creation latency
