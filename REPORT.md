# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-17 13:35:31 UTC
**Commit:** 15efc8dc6220b630dba78ec51d30a98b179047b4

## Results

test pagination_offset_deep_page ... bench:       34614 ns/iter (+/- 929)

test pagination_keyset_deep_page ... bench:          30 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         148 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         159 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        5855 ns/iter (+/- 91)

