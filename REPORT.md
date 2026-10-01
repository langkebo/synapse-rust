# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 23:19:50 UTC
**Commit:** 0e615d7ee31070f6ec6fec689574f15c036bc898

## Results

test pagination_offset_deep_page ... bench:       55483 ns/iter (+/- 141)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         281 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         286 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7626 ns/iter (+/- 21)

