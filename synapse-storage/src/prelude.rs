//! Backward-compatibility prelude for `synapse-storage`.
//!
//! Provides a single glob-import point that re-exports the domain-grouped
//! storage types. Consumers can replace many individual imports with:
//!
//! ```ignore
//! use synapse_storage::prelude::*;
//! ```
//!
//! Currently covers the domain groups created in P1/P2/P4:
//! - `admin::*` (admin_federation, admin_media, audit)
//! - `auth::*` (user, device, token, threepid, captcha, openid_token)
//! - `e2ee::*` (dehydrated_device, e2ee_audit)
//!
//! Types that are not yet grouped into a domain (e.g. `room`, `media`,
//! `presence`) remain accessible via the crate root or their module path;
//! they will be folded into the prelude in P7 once their domain groups exist.

#[doc(no_inline)]
pub use crate::admin::*;
#[doc(no_inline)]
pub use crate::auth::*;
#[doc(no_inline)]
pub use crate::e2ee::*;
