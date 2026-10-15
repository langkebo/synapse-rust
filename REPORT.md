# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 07:15:54 UTC
**Commit:** b4c845b2081406b3b1a7c6f57cc65cabaf6d4d20

## Results

test pagination_offset_deep_page ... bench:       70033 ns/iter (+/- 98)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         283 ns/iter (+/- 6)

test state_resolution_chain_100 ... bench:         277 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7396 ns/iter (+/- 5)

