# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-04 16:00:34 UTC
**Commit:** 326cfe63515c322ca323db3ebb343164f5ba9979

## Results

test pagination_offset_deep_page ... bench:       69339 ns/iter (+/- 7641)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 21)

test state_resolution_chain_100 ... bench:         278 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7042 ns/iter (+/- 5)

