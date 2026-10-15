# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 05:53:01 UTC
**Commit:** 426e8299975356d21195e7de341f9a40e2af9685

## Results

test pagination_offset_deep_page ... bench:       72299 ns/iter (+/- 461)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         279 ns/iter (+/- 6)

test state_resolution_chain_100 ... bench:         276 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7139 ns/iter (+/- 36)

