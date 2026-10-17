# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-17 10:50:44 UTC
**Commit:** 00a94e95a0f39fc6cbb808c1ac9b85ede69bb9f6

## Results

test pagination_offset_deep_page ... bench:       54142 ns/iter (+/- 187)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         271 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7265 ns/iter (+/- 6)

