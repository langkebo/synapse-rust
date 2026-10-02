//! The single implementation of "the room's full state, cached".
//!
//! # Why this exists
//!
//! Two readers want the same value and nothing else:
//!
//! * sliding-sync builds `required_state` sections for parked clients;
//! * the third-party event admission gate (`check_event_allowed`) needs the state
//!   the rules are evaluated against.
//!
//! They used to disagree: sliding-sync read `room_state:{room_id}` from the cache,
//! while the gate called `EventReader::get_state_events` **uncached** — so with any
//! rule registered, every event write materialised the room's whole state. That is
//! one responsibility with two implementations (AGENTS.md 铁律 2), and the gate's
//! copy was the expensive one.
//!
//! Both now go through [`cached_room_state`], so they share a single cache entry,
//! a single key, and a single TTL.
//!
//! # Invalidation contract
//!
//! Whoever writes a **state** event must delete `room_state:{room_id}`. Audited
//! 2026-10-03 across every production `state_key: Some(...)` write site: the
//! central delete lives in
//! `room::messaging::events::{create_event, create_event_with_graph, create_outlier_event}`
//! (`if state_key.is_some() { delete … }`), which covers `set_pinned_event_ids`,
//! `upgrade_room`'s `m.room.tombstone` and `friend_room_service::send_state_event_inner`
//! because all three go through those methods; the membership / moderation /
//! federation flows delete explicitly, and the room-creation sequence deletes at
//! the end of `room::lifecycle::create`. No bypassing writer was found.
//!
//! Because admission decisions now depend on this entry, **any new state-write
//! path must invalidate it** — otherwise a rule evaluates against stale state.

use synapse_cache::CacheManager;
use synapse_storage::event::{EventReader, StateEvent};

/// TTL of the `room_state:{room_id}` entry, in seconds.
pub const ROOM_STATE_CACHE_TTL_SECS: u64 = 300;

/// Cache key for the full state of `room_id`.
pub fn room_state_cache_key(room_id: &str) -> String {
    format!("room_state:{room_id}")
}

/// Full room state for `room_id`, served from `cache` when present.
///
/// The read is best-effort cached: a cache write failure is non-fatal, because the
/// freshly fetched value is still returned to the caller.
pub async fn cached_room_state(
    cache: &CacheManager,
    reader: &dyn EventReader,
    room_id: &str,
) -> Result<Vec<StateEvent>, sqlx::Error> {
    let key = room_state_cache_key(room_id);

    if let Ok(Some(cached)) = cache.get::<Vec<StateEvent>>(&key).await {
        return Ok(cached);
    }

    let fetched = reader.get_state_events(room_id).await?;
    let _ = cache.set(&key, &fetched, ROOM_STATE_CACHE_TTL_SECS).await;
    Ok(fetched)
}
