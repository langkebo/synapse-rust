//! Event services domain group.
//!
//! Re-exports event-related service modules (event_broadcaster_trait,
//! event_notifier, event_report_service) under a single namespace so that new
//! event services can be added here without touching `lib.rs`.
//!
//! Consumers may use either:
//! - `synapse_services::event::EventNotifier` (preferred, grouped path)
//! - `synapse_services::EventNotifier` (legacy flat path, via `pub use event::*` in lib.rs)
//!
//! Note: `EventBroadcaster` is also re-exported at the crate root via the
//! federation sibling-crate bridge import (`pub use federation::{...
//! EventBroadcaster}`); the explicit bridge import takes precedence over the
//! glob here, so both paths resolve to the same underlying trait.

pub use crate::event_broadcaster_trait::{BroadcastError, EventBroadcaster};
pub use crate::event_notifier::{EventNotifier, EventNotifyKind, EventNotifyMessage};
pub use crate::event_report_service::EventReportService;
