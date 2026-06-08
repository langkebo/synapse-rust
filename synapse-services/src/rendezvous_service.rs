//! Service-layer entry point for the rendezvous feature.
//!
//! The route module ([`crate::web::routes::rendezvous`]) historically
//! consumed storage types and called the storage struct directly. To
//! keep the `route → service → storage` layering enforceable, this
//! module re-exports the request / response DTOs from the storage
//! module so the route can depend on the service module only.
//!
//! Higher-level business rules (such as intent/transport validation
//! and policy enforcement) will be added here in later batches; for
//! now the only responsibility is the public re-export.

pub use synapse_storage::rendezvous::{
    CreateRendezvousSessionParams, RendezvousCode, RendezvousIntent, RendezvousLoginFinish, RendezvousLoginStart,
    RendezvousLoginUser, RendezvousMessage, RendezvousMessageStorage, RendezvousSession, RendezvousTransport,
    StoredRendezvousMessage,
};
