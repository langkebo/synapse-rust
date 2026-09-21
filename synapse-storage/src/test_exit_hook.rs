//! Test-build-only registration of the schema janitor's process-exit drain (B').
//!
//! `synapse_common::test_schema_guard::drain_schemas_at_exit` must be installed
//! with `libc::atexit` by every **test binary** that can register schemas; the
//! registration cannot live in the dependency (a `#[cfg(test)]` item in
//! `synapse-common` is invisible to this crate's test build) and must not live in
//! this crate's production build (the `unsafe` would then show up in
//! cargo-geiger's production scan). Hence this module, which is compiled only for
//! `#[cfg(test)]`, and the calls placed in this crate's shared pool fixtures.
//!
//! The guard that keeps the set of registering crates in sync with the set of
//! crates that register schemas: `tests/unit/ci_test_scope_tests.rs` →
//! `every_db_test_binary_registers_the_exit_drain`.
//!
//! The module is already gated by `#[cfg(test)]` at its `mod` declaration in
//! `lib.rs`. An inner `#![cfg(test)]` here would be a **duplicate attribute**,
//! which clippy rejects (`duplicated_attributes`) under `-D warnings`.

use std::sync::Once;

/// Installs the exit drain once per process.
pub(crate) fn ensure() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: `drain_schemas_at_exit` is a plain `extern "C" fn` taking no
        // arguments, capturing no state and returning nothing; its body only
        // flips an atomic and joins the janitor thread with a bounded wait, all
        // of which is sound to run during process exit.
        unsafe { libc::atexit(synapse_common::test_schema_guard::drain_schemas_at_exit) };
    });
}
