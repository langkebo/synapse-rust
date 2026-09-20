# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 23:38:46 UTC
**Commit:** 6009ff85ce0fdad9be89c8819e9ac4b96f2e6d17

## Results

test pagination_offset_deep_page ... bench:       54823 ns/iter (+/- 115)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         280 ns/iter (+/- 12)

test auth_chain_build_10 ... bench:        7235 ns/iter (+/- 41)

