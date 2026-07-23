pub mod gateway;
pub mod providers;
pub mod queue;
pub mod service;

// Push domain group — re-exports push::service notification types under `push::`.
pub use service::{NotificationPayload, PushNotificationService, PushRuleResult, SendNotificationRequest};
