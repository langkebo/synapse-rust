//! Test-only configuration builder for `ServiceContainer`.
//!
//! Thin facade re-exporting the canonical implementation from
//! `synapse_services::test_config`. This module is only compiled under
//! the `test-utils` feature.
//!
//! Note: We use `feature = "test-utils"` (not `any(test, feature = "test-utils")`)
//! because `synapse_services::test_config` is itself gated the same way, and the
//! `test` cfg does not propagate to dependency crates. Using `any(test, ...)` here
//! would compile this facade without compiling the canonical module it re-exports.
//!
//! The `#[cfg(feature = "test-utils")]` gate lives on the module declaration in
//! `super::mod.rs`; we intentionally do NOT repeat it as an inner attribute to
//! avoid clippy::duplicated_attributes.

pub use synapse_services::test_config::*;
