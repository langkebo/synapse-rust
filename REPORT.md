# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 04:18:39 UTC
**Commit:** b1d66865854fad85412db9f6c1f7ccdbe112532f

## Results

test pagination_offset_deep_page ... bench:       54005 ns/iter (+/- 41)

test pagination_keyset_deep_page ... bench:          55 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         285 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         277 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6603 ns/iter (+/- 9)

