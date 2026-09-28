# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-28 19:59:40 UTC
**Commit:** 0e62273f54fd0b7561347672b224ac0b89460c2b

## Results

test pagination_offset_deep_page ... bench:       71420 ns/iter (+/- 696)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         269 ns/iter (+/- 10)

test state_resolution_chain_100 ... bench:         278 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6579 ns/iter (+/- 44)

