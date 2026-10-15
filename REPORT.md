# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 10:20:23 UTC
**Commit:** e792b086a99bafee5fb1b5def46f58fc788e5de2

## Results

test pagination_offset_deep_page ... bench:       42926 ns/iter (+/- 109)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         211 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         211 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        5981 ns/iter (+/- 18)

