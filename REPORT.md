# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 02:25:42 UTC
**Commit:** cc59ffb422fb13607898c35b48cabdd4e6345d07

## Results

test pagination_offset_deep_page ... bench:       55542 ns/iter (+/- 132)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         297 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         309 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        6965 ns/iter (+/- 11)

