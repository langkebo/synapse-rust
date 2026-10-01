# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 15:54:58 UTC
**Commit:** 1a8e5af77b1c4ba3f14ca885233b3efc0f847e4c

## Results

test pagination_offset_deep_page ... bench:       55667 ns/iter (+/- 202)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         289 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         291 ns/iter (+/- 4)

test auth_chain_build_10 ... bench:        7665 ns/iter (+/- 32)

