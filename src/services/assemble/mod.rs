//! Domain-grouped assembly helpers used by `ServiceContainer::new`.
//!
//! This module is part of the M-1 refactor that splits the previous
//! 1431-line `services/container.rs` god file into focused, callable
//! helpers. The intent is to keep the public
//! [`ServiceContainer`](super::ServiceContainer) surface unchanged
//! while moving the per-domain wiring (E2EE, room/sync, federation,
//! admin/support) into self-contained sub-modules.
//!
//! Each sub-module exposes a single `assemble_*` entry point that
//! returns an intermediate struct consumed by `ServiceContainer::new`.

pub mod admin;
pub mod e2ee;
pub mod federation;
pub mod room_sync;

pub use admin::assemble_admin_support;
pub use e2ee::assemble_e2ee;
pub use federation::assemble_federation;
pub use room_sync::assemble_room_and_sync;
