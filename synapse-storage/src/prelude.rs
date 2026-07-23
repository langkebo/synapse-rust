//! Backward-compatibility prelude for `synapse-storage`.
//!
//! Provides a single glob-import point that re-exports ALL domain-grouped
//! storage types. Consumers can replace many individual imports with:
//!
//! ```ignore
//! use synapse_storage::prelude::*;
//! ```

#[doc(no_inline)]
pub use crate::account::*;
#[doc(no_inline)]
pub use crate::admin::*;
#[doc(no_inline)]
pub use crate::application::*;
#[doc(no_inline)]
pub use crate::auth::*;
#[doc(no_inline)]
pub use crate::e2ee::*;
#[doc(no_inline)]
pub use crate::event::*;
#[doc(no_inline)]
pub use crate::infra::*;
#[doc(no_inline)]
pub use crate::media::*;
#[doc(no_inline)]
pub use crate::moderation::*;
#[doc(no_inline)]
pub use crate::oidc::*;
#[doc(no_inline)]
pub use crate::push::*;
#[doc(no_inline)]
pub use crate::room::*;
#[doc(no_inline)]
pub use crate::space::*;
#[doc(no_inline)]
pub use crate::sync::*;

#[cfg(feature = "openclaw-routes")]
#[doc(no_inline)]
pub use crate::ai::*;

#[cfg(feature = "voip-tracking")]
#[doc(no_inline)]
pub use crate::rtc::*;
