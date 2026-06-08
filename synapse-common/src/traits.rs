use crate::error::ApiError;

/// Dehydrated device provider trait — breaks e2ee <-> services circular dependency.
///
/// e2ee modules (cross_signing, ssss) depend on this trait instead of
/// `DehydratedDeviceService` directly, allowing the concrete implementation
/// to live in the services layer without creating a circular crate dependency.
#[async_trait::async_trait]
pub trait DehydratedDeviceProvider: Send + Sync {
    async fn get_dehydrated_device(&self, user_id: &str) -> Result<Option<serde_json::Value>, ApiError>;
    async fn put_dehydrated_device(&self, user_id: &str, data: serde_json::Value) -> Result<String, ApiError>;
    async fn delete_dehydrated_device(&self, user_id: &str, device_id: &str) -> Result<(), ApiError>;
}

/// Friend room provider trait — breaks federation <-> services circular dependency.
///
/// The federation friend module depends on this trait instead of
/// `FriendRoomService` directly, allowing the concrete implementation
/// to live in the services layer without creating a circular crate dependency.
#[async_trait::async_trait]
pub trait FriendRoomProvider: Send + Sync {
    /// Handle an incoming friend request from a remote federated server.
    async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: serde_json::Value,
    ) -> Result<(), ApiError>;
}

// =============================================================================
// EventBroadcaster — unified event broadcasting abstraction
// =============================================================================

/// A generic event broadcasting interface.
///
/// All three broadcast implementations in this codebase implement this trait,
/// providing a uniform `publish` + `subscriber_count` surface.
///
/// # Implementors
///
/// * `EventNotifier` — local sync wake-up (room / user `Notify` + Redis fan-out)
/// * `federation::EventBroadcaster` — federation outbound (PDU/EDU batching + retry)
/// * `WorkerBus` — inter-worker messaging (replication commands)
///
/// # Design note
///
/// The three implementations serve fundamentally different domains (local
/// wake-up vs. federation transport vs. worker replication), so they are **not**
/// merged into a single concrete type. Instead, this trait captures their
/// shared *publish* contract so that callers can depend on the abstraction
/// when appropriate, while each implementation retains its domain-specific
/// optimisations.
pub trait EventBroadcaster: Send + Sync {
    /// The message type produced by this broadcaster.
    type Message: Send + Sync + Clone + std::fmt::Debug;

    /// Publish a message to all active subscribers.
    ///
    /// Returns `Ok(())` on success or an error describing why the publish
    /// failed (e.g. not connected, encoding error).
    ///
    /// This is `async` because some implementations (federation, worker bus)
    /// perform I/O during publish (network send, Redis pub/sub, etc.).
    /// Implementations that are purely in-memory (e.g. `EventNotifier`) can
    /// complete synchronously within the future.
    fn broadcast_publish(
        &self,
        message: Self::Message,
    ) -> impl std::future::Future<Output = Result<(), BroadcastError>> + Send;

    /// Return the number of currently active subscribers / waiters.
    fn broadcast_subscriber_count(&self) -> usize;
}

/// Error type shared across all [`EventBroadcaster`] implementations.
#[derive(Debug, thiserror::Error)]
pub enum BroadcastError {
    /// The broadcaster is not connected / not initialised.
    #[error("Not connected")]
    NotConnected,

    /// Failed to encode / serialise the message.
    #[error("Encoding failed: {0}")]
    EncodingFailed(String),

    /// A transport-level error (network, Redis, etc.).
    #[error("Transport error: {0}")]
    Transport(String),

    /// The message was rejected due to back-pressure or a full buffer.
    #[error("Channel full: {0}")]
    ChannelFull(String),

    /// Catch-all for implementation-specific errors.
    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for BroadcastError {
    fn from(e: serde_json::Error) -> Self {
        BroadcastError::EncodingFailed(e.to_string())
    }
}
