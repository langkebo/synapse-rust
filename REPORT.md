# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 07:55:38 UTC
**Commit:** 6ef02fa1e21cdb63faf0992b72502ee2aec6b4b1

## Results

test pagination_offset_deep_page ... bench:       70971 ns/iter (+/- 7874)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         274 ns/iter (+/- 14)

test state_resolution_chain_100 ... bench:         280 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6516 ns/iter (+/- 10)

