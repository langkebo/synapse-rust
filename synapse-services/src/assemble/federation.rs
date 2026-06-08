//! Federation domain assembly — namespace sub-module.
//!
//! This file is part of the M-1 refactor that splits the previous
//! 1431-line `services/container.rs` god file into focused, callable
//! helpers. The canonical `assemble_federation` implementation still
//! lives in `crate::container` for now.

pub use crate::container::assemble_federation;
