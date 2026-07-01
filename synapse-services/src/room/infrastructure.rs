use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared infrastructure injected into RoomService sub-services.
///
/// Bundles the four optional backend services that are wired after
/// RoomService construction (because they depend on federation/admin
/// services being built first). Sub-services that need these hold
/// clones of the relevant RwLock rather than reaching through a
/// RoomService back-reference.
#[derive(Clone)]
pub struct RoomInfrastructure {
    pub event_broadcaster: Arc<RwLock<Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>>>,
    pub app_service_manager: Arc<RwLock<Option<Arc<crate::application_service::ApplicationServiceManager>>>>,
    pub key_rotation_manager: Arc<RwLock<Option<Arc<synapse_federation::KeyRotationManager>>>>,
    pub federation_client: Arc<RwLock<Option<Arc<synapse_federation::FederationClient>>>>,
}

impl RoomInfrastructure {
    /// Create a new `RoomInfrastructure` with all fields set to `None`.
    ///
    /// Values are typically provided after construction via the dedicated
    /// setter methods (e.g. `set_event_broadcaster`).
    pub fn new() -> Self {
        Self {
            event_broadcaster: Arc::new(RwLock::new(None)),
            app_service_manager: Arc::new(RwLock::new(None)),
            key_rotation_manager: Arc::new(RwLock::new(None)),
            federation_client: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the event broadcaster used to push local events to remote servers.
    pub async fn set_event_broadcaster(&self, b: Arc<synapse_federation::event_broadcaster::EventBroadcaster>) {
        *self.event_broadcaster.write().await = Some(b);
    }

    /// Set the application service manager used to dispatch events to
    /// application services (bridges).
    pub async fn set_app_service_manager(&self, m: Arc<crate::application_service::ApplicationServiceManager>) {
        *self.app_service_manager.write().await = Some(m);
    }

    /// Set the key rotation manager used to sign outbound PDUs before
    /// federating them.
    pub async fn set_key_rotation_manager(&self, m: Arc<synapse_federation::KeyRotationManager>) {
        *self.key_rotation_manager.write().await = Some(m);
    }

    /// Set the federation client used for outbound federation requests
    /// (make_join, send_join, make_leave, send_leave, invite, etc.).
    pub async fn set_federation_client(&self, c: Arc<synapse_federation::FederationClient>) {
        *self.federation_client.write().await = Some(c);
    }
}

impl Default for RoomInfrastructure {
    fn default() -> Self {
        Self::new()
    }
}
