# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 02:18:12 UTC
**Commit:** 3b19ce95a0a5e0a763f66cd4741edd377d72a7aa

## Results

test pagination_offset_deep_page ... bench:       54478 ns/iter (+/- 121)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         267 ns/iter (+/- 24)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7298 ns/iter (+/- 7)

