/// The `gateway` module.
pub mod gateway;
/// The `providers` module.
pub mod providers;
/// The `queue` module.
pub mod queue;
/// The `service` module.
pub mod service;

// Push domain group — re-exports push::service notification types under `push::`.
pub use service::{NotificationPayload, PushNotificationService, SendNotificationRequest};

// P7.4 — additional push-domain service re-export (previously a root module only).
pub use crate::client_push_service::*;
