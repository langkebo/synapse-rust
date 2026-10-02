# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 09:11:56 UTC
**Commit:** 40bd1f1e1bf7ae37e8aadd635a65accf0d44badf

## Results

test pagination_offset_deep_page ... bench:       70554 ns/iter (+/- 79)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         272 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         276 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6437 ns/iter (+/- 18)

