pub use synapse_federation::client;
/// The `edu` module.
pub mod edu;
pub use synapse_federation::event_auth;
pub use synapse_federation::event_broadcaster;
/// The `friend` module.
#[cfg(feature = "friends")]
pub mod friend;
pub use synapse_federation::key_rotation;
pub use synapse_federation::signing;

pub use client::FederationClient;
pub use edu::{EduDispatcher, EduProcessResult, EduType};
pub use event_auth::EventAuthChain;
pub use event_broadcaster::EventBroadcaster;
#[cfg(feature = "friends")]
pub use friend::*;
pub use key_rotation::KeyRotationManager;
