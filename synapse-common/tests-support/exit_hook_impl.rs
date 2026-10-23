//! Test-build-only registration of the schema janitor's process-exit drain (B').
//!
//! # This file is the single implementation
//!
//! Every test build that can register schemas `#[path = ...]`-includes this
//! file instead of carrying its own copy:
//!
//! | consumer | `mod` declaration site |
//! |---|---|
//! | `synapse-common` | `src/test_schema_guard.rs` (`#[cfg(test)]`) |
//! | `synapse-rust` (lib tests) | `src/lib.rs` (`#[cfg(test)]`) |
//! | `synapse-rust` (`tests/unit`, `tests/integration`) | `tests/common/mod.rs` |
//! | `synapse-services` | `src/lib.rs` (`#[cfg(test)]`) |
//! | `synapse-storage` | `src/lib.rs` (`#[cfg(test)]`) |
//! | `synapse-test-utils` | `src/lib.rs` (`#[cfg(test)]`) |
//!
//! Before 2026-10-23 the five per-crate rows were byte-identical copies (same
//! md5), so every change to the registration had to be made five times, and
//! `scripts/ci/geiger_baseline.json` could only itemise one of them.
//!
//! # Why it lives outside `src/`
//!
//! `git grep -n unsafe synapse-common/src` must stay empty: this crate ships no
//! `unsafe` at all. `cargo geiger` (production scan, no `--include-tests`)
//! therefore reports zero production units, while
//! `cargo geiger --include-tests` still sees the call — the unit lands in the
//! test-only delta. Runtime behaviour is unchanged: the very same
//! `drain_schemas_at_exit` runs at process exit.
//!
//! # Why `synapse_common::` and not `crate::`
//!
//! This file is expanded inside the *consumer's* crate, where `crate::` would
//! point at the consumer. `synapse-common` adds
//! `extern crate self as synapse_common;` to its root so the same absolute path
//! also resolves inside itself.
//!
//! # Guards
//!
//! * `tests/unit/ci_test_scope_tests.rs` → `every_db_test_binary_registers_the_exit_drain`
//!   keeps the *set* of registering crates in sync with the set of crates that
//!   register schemas.
//! * `tests/unit/ci_test_scope_tests.rs` → `exit_hook_has_a_single_implementation`
//!   keeps this the only implementation. It counts `libc::atexit` **call sites**,
//!   not filenames, so adding a copy back under a different name does not evade it.
//!
//! `ensure` is `pub(crate)` on purpose: the consumers declare it at different
//! module depths (`crate::test_exit_hook` at a crate root,
//! `test_schema_guard::test_exit_hook` in `synapse-common`), and `pub(crate)` is
//! the widest form valid in both — `pub(super)` would name *different* scopes at
//! the two positions.

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
