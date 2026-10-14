# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 12:41:43 UTC
**Commit:** 0fb7fa8e0f035eed3a386a1cd0aba64eafb653b7

## Results

test pagination_offset_deep_page ... bench:       54290 ns/iter (+/- 31)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 28)

test state_resolution_chain_100 ... bench:         274 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6705 ns/iter (+/- 4)

