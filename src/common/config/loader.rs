//! Configuration loading and environment variable resolution.
//!
//! The `impl Config` blocks (`load`, `resolve_env_variables`, `validate`)
//! now live in `synapse_common::config` and are re-exported via
//! `pub use synapse_common::config::Config;` in `mod.rs`. This file is
//! kept only to preserve the `mod loader;` declaration for any future
//! root-specific loading helpers.
