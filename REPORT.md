# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-18 13:22:46 UTC
**Commit:** 9d2f152bb7637430d9a6c99be5bda8c571d594c7

## Results

test pagination_offset_deep_page ... bench:       54093 ns/iter (+/- 112)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         274 ns/iter (+/- 16)

test state_resolution_chain_100 ... bench:         278 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7069 ns/iter (+/- 14)

