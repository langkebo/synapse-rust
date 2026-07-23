// =============================================================================
// L0 — Core Matrix storage re-exports (always compiled).
// All re-exports resolve directly to synapse_storage. Local facade files and
// directory modules have been removed.
//
// All src/ consumers now import directly from synapse_storage. The remaining
// re-exports below serve test suites, benchmark harnesses, and binary crates
// (schema_health_check, synapse_worker) that still route through this facade.
// =============================================================================
pub use synapse_storage::application_service;
pub use synapse_storage::audit;
pub use synapse_storage::background_update;
#[cfg(feature = "beacons")]
pub use synapse_storage::beacon;
pub use synapse_storage::device;
pub use synapse_storage::event;
pub use synapse_storage::event_report;
pub use synapse_storage::feature_flags;
pub use synapse_storage::federation_blacklist;
pub use synapse_storage::filter;
#[cfg(feature = "friends")]
pub use synapse_storage::friend_room;
pub use synapse_storage::invite_blocklist;
pub use synapse_storage::maintenance;
#[cfg(feature = "voip-tracking")]
pub use synapse_storage::matrixrtc;
pub use synapse_storage::membership;
pub use synapse_storage::moderation;
pub use synapse_storage::module;
pub use synapse_storage::monitoring;
#[cfg(feature = "openclaw-routes")]
pub use synapse_storage::openclaw;
pub use synapse_storage::openid_token;
pub use synapse_storage::presence;
#[cfg(feature = "privacy-ext")]
pub use synapse_storage::privacy;
pub use synapse_storage::qr_login;
pub use synapse_storage::refresh_token;
pub use synapse_storage::registration_token;
pub use synapse_storage::relations;
pub use synapse_storage::retention;
pub use synapse_storage::room;
pub use synapse_storage::room_summary;
pub use synapse_storage::room_tag;
pub use synapse_storage::schema_health_check;
pub use synapse_storage::schema_validator;
#[cfg(feature = "server-notifications")]
pub use synapse_storage::server_notification;
pub use synapse_storage::sliding_sync;
pub use synapse_storage::space;
pub use synapse_storage::state_groups;
pub use synapse_storage::sticky_event;
pub use synapse_storage::thread;
pub use synapse_storage::threepid;
pub use synapse_storage::token;
pub use synapse_storage::user;
#[cfg(feature = "widgets")]
pub use synapse_storage::widget;

pub use synapse_storage::{initialize_database, Database};

// Domain group re-exports — consumers can use `synapse_rust::storage::room::Type`
// or `synapse_rust::storage::auth::Type` instead of flat module paths.
// Only domain modules not already re-exported above as individual modules are
// listed here (event, moderation, room, space are already re-exported above).
#[cfg(feature = "openclaw-routes")]
pub use synapse_storage::ai;
#[cfg(feature = "voip-tracking")]
pub use synapse_storage::rtc;
pub use synapse_storage::{account, admin, application, auth, e2ee, infra, media, oidc, push, sync};
