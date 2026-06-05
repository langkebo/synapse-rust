//! E2EE domain assembly — namespace sub-module.
//!
//! This file is part of the M-1 refactor that splits the previous
//! 1431-line `services/container.rs` god file into focused, callable
//! helpers. The canonical `assemble_e2ee` implementation still lives
//! in `crate::services::container` for now.

pub use crate::services::container::assemble_e2ee;
