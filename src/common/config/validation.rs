//! Configuration validation logic.
//!
//! The `impl Config` block (`validate`) now lives in
//! `synapse_common::config::validation` and is re-exported via
//! `pub use synapse_common::config::Config;` in `mod.rs`. This file is
//! kept only to preserve the `mod validation;` declaration for any
//! future root-specific validation helpers.
