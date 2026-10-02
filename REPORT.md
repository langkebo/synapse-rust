# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 00:18:14 UTC
**Commit:** 7b54e01fd7467eca48c78f2929fb04a3d955c859

## Results

test pagination_offset_deep_page ... bench:      140348 ns/iter (+/- 70)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         212 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         226 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        9863 ns/iter (+/- 23)

