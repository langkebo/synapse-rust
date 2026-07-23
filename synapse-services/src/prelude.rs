//! Backward-compatibility prelude for `synapse-services`.
//!
//! Provides a single glob-import point that re-exports ALL domain-grouped
//! service types. Consumers can replace many individual imports with:
//!
//! ```ignore
//! use synapse_services::prelude::*;
//! ```

#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::account::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::admin::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::application::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::event::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::identity::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::infra::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::media::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::push::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::room::*;
#[doc(no_inline)]
#[allow(ambiguous_glob_reexports)]
pub use crate::sync::*;
