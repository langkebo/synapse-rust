use std::sync::Arc;

/// Shared infrastructure injected into RoomService sub-services.
///
/// Bundles the four optional backend services that federation-adjacent room
/// operations depend on. Each is `Some` in a fully-wired homeserver and `None`
/// in test/minimal deployments that don't exercise federation or application
/// services. Values are supplied once at construction (via `RoomServiceConfig`)
/// and are immutable thereafter — there is no post-construction mutation.
#[derive(Clone)]
pub struct RoomInfrastructure {
    pub event_broadcaster: Option<Arc<synapse_federation::event_broadcaster::EventBroadcaster>>,
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    pub key_rotation_manager: Option<Arc<synapse_federation::KeyRotationManager>>,
    pub federation_client: Option<Arc<dyn synapse_federation::client_api::FederationClientApi>>,
}
