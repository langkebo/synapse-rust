# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-05 01:47:52 UTC
**Commit:** b5c83355c4b86e1cf8d0cbf57919cda9a75db6f1

## Results

test pagination_offset_deep_page ... bench:       55730 ns/iter (+/- 134)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         269 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7476 ns/iter (+/- 10)

