// Thin re-export from synapse-common (canonical implementation).
// Minor diffs (2026-06-11): import paths resolve internally; TOKEN_HASH_SECRET
// fallback behavior differs (root: unwrap_or_else, sub: expect) — sub-crate is
// stricter, root is more forgiving in dev. Behaviorally equivalent in production.
pub use synapse_common::crypto::*;