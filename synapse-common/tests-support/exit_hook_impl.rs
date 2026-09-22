//! Implementing half of `test_schema_guard::test_exit_hook` — see the `include!`
//! site for why this lives outside `src/`.
//!
//! B' design: the janitor's process-exit drain is registered by each **test
//! binary**, so the `unsafe { libc::atexit(..) }` call is compiled only into test
//! builds. `cargo geiger` (production scan, no `--include-tests`) therefore stops
//! counting it, while `cargo geiger --include-tests` still sees it — i.e. the
//! unit moves from "production unsafe" to the test-only delta. Runtime behaviour
//! is unchanged: the very same `drain_schemas_at_exit` runs at process exit.

use std::sync::Once;

/// Installs the exit drain once per process.
pub(super) fn ensure() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: `drain_schemas_at_exit` is a plain `extern "C" fn` taking no
        // arguments, capturing no state and returning nothing; its body only
        // flips an atomic and joins the janitor thread with a bounded wait, all
        // of which is sound to run during process exit.
        unsafe { libc::atexit(crate::test_schema_guard::drain_schemas_at_exit) };
    });
}
