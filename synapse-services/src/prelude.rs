//! Backward-compatibility prelude for `synapse-services`.
//!
//! Provides a single glob-import point that re-exports the domain-grouped
//! service types. Consumers can replace many individual imports with:
//!
//! ```ignore
//! use synapse_services::prelude::*;
//! ```
//!
//! Currently covers the domain groups created in P0/P5:
//! - `admin::*` (admin_audit, admin_federation, admin_media, admin_registration,
//!   admin_security, admin_server, admin_token, admin_user services)
//! - `sync::*` (sync_service, sliding_sync_service, sync_helpers)
//!
//! Types that are not yet grouped into a domain (e.g. `room`, `media`,
//! `push`) remain accessible via the crate root or their module path; they
//! will be folded into the prelude in P7 once their domain groups exist.

#[doc(no_inline)]
pub use crate::admin::*;
#[doc(no_inline)]
pub use crate::sync::*;
