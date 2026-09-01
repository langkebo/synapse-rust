// Thin shell — route cache consumers to the synapse-cache crate.
// The glob re-exports everything synapse-cache publicly exposes; the
// 5 specific sub-module re-exports above were redundant.
pub use synapse_cache::*;
