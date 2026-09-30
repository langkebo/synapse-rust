# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 04:06:40 UTC
**Commit:** d8a7a672c17a831618f39e6402b6d9898a1f9c42

## Results

test pagination_offset_deep_page ... bench:       54903 ns/iter (+/- 130)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 22)

test state_resolution_chain_100 ... bench:         277 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7267 ns/iter (+/- 10)

