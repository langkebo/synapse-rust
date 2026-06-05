//! Service-layer entry point for room event operations.
//!
//! The route modules under [`crate::web::routes::handlers::room`] import
//! [`crate::storage::event::CreateEventParams`] and other DTOs. This
//! module re-exports the storage-level DTOs so the route can depend
//! on the service module rather than `crate::storage::*`, preserving
//! the `route → service → storage` layering.
//!
//! Higher-level event business rules (validation, normalisation,
//! state-update propagation) will be moved into a real
//! `EventService` in later batches; the current scope is the
//! type-re-export shim only.

pub use crate::storage::event::{CreateEventParams, EventStorage, RoomEvent, StateEvent};
