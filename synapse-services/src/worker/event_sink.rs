//! The **write** side of the worker event bus (`worker_events`).
//!
//! # Why this exists
//!
//! `worker_events` is the replication stream a worker polls to catch up on room
//! events: `GET .../worker/events` → `WorkerManager::get_events_since` →
//! `WorkerStoreApi::get_events_since`. The read side shipped without a write side —
//! `WorkerManager::add_event` had **no caller anywhere in the repository** — so the
//! endpoint returned an empty list forever, silently (see
//! `docs/synapse-rust-vs-synapse-comparison.md` §18.7 P-5). This module is that
//! missing write side.
//!
//! # Contract
//!
//! * **Best effort.** The room event is already committed when this runs, so a
//!   publish failure is logged and swallowed: it must never fail the write that
//!   triggered it. Losing a publish degrades to "workers do not see this event";
//!   failing the write would corrupt a user-visible operation.
//! * **Idempotent.** `worker_events` has `UNIQUE (event_id)` and the same event can
//!   legitimately be written more than once (federation backfill, retries). The
//!   storage insert is `ON CONFLICT (event_id) DO UPDATE`, so a republish returns
//!   the original row and preserves its `stream_id`.
//! * **Worker mode only.** Callers wire this sink only when `worker.enabled` is
//!   set. In a single-process deployment the bus has no reader, so publishing
//!   would add one INSERT per room event for nothing.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, warn};

use synapse_storage::event::RoomEvent;

use crate::worker::WorkerManager;

/// Sink for room events that must reach workers.
///
/// Implementations are **best effort** and never surface a failure to the caller;
/// see the module contract.
#[async_trait]
pub trait WorkerEventSink: Send + Sync {
    /// Publish `event` to the worker bus.
    async fn publish_room_event(&self, event: &RoomEvent);
}

/// [`WorkerEventSink`] backed by the in-process [`WorkerManager`].
///
/// Goes through `WorkerManager` rather than the raw store so that workers which
/// are currently connected are pushed the new row immediately
/// (`WorkerManager::broadcast_event`). Workers that are not connected still pick
/// it up from `worker_events` on their next poll, because the row is persisted
/// either way.
pub struct WorkerManagerEventSink {
    manager: Arc<WorkerManager>,
}

impl WorkerManagerEventSink {
    /// Wraps `manager`.
    pub fn new(manager: Arc<WorkerManager>) -> Self {
        Self { manager }
    }
}

impl std::fmt::Debug for WorkerManagerEventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerManagerEventSink").finish_non_exhaustive()
    }
}

#[async_trait]
impl WorkerEventSink for WorkerManagerEventSink {
    async fn publish_room_event(&self, event: &RoomEvent) {
        match self
            .manager
            .add_event(
                &event.event_id,
                &event.event_type,
                Some(event.room_id.as_str()),
                Some(event.user_id.as_str()),
                event.content.clone(),
            )
            .await
        {
            Ok(published) => debug!(
                event_id = %published.event_id,
                stream_id = published.stream_id,
                room_id = %event.room_id,
                "published room event to the worker bus"
            ),
            Err(error) => warn!(
                error = %error,
                event_id = %event.event_id,
                room_id = %event.room_id,
                "failed to publish room event to the worker bus — workers will not see this event \
                 until it is re-published"
            ),
        }
    }
}
