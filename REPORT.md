# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-28 13:27:16 UTC
**Commit:** 531c7e7bd822a2be7613f627c0c5e9904e4f0f4b

## Results

test pagination_offset_deep_page ... bench:       54333 ns/iter (+/- 176)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         268 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         273 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6616 ns/iter (+/- 12)

