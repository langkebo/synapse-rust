//! Event services domain group.
//!
//! Re-exports event-related service modules (event_broadcaster_trait,
//! event_notifier, event_report_service) under a single namespace so that new
//! event services can be added here without touching `lib.rs`.
//!
//! Consumers use the grouped path:
//! - `synapse_services::event::EventNotifier`
//!
//! Note: `EventBroadcaster` is also re-exported at the crate root via the
//! federation sibling-crate bridge import (`pub use federation::{...
//! EventBroadcaster}`); the explicit bridge import takes precedence over the
//! glob here, so both paths resolve to the same underlying trait.

pub use crate::event_broadcaster_trait::{BroadcastError, EventBroadcaster};
pub use crate::event_notifier::{EventNotifier, EventNotifyKind, EventNotifyMessage};
pub use crate::event_report_service::EventReportService;

// Event DTOs the HTTP layer needs. Re-exported through the service layer so
// `src/web` depends on `synapse-services` instead of reaching into
// `synapse-storage` directly (A2 / B4-4; see scripts/ci/check_web_layering.py).
pub use synapse_storage::event::{CreateEventParams, RoomEvent, StateEvent};
