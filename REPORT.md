# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-19 03:25:51 UTC
**Commit:** 0c14f1d000e7be41cfb3313cb11833f3a561c7cc

## Results

test pagination_offset_deep_page ... bench:       32304 ns/iter (+/- 182)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         144 ns/iter (+/- 5)

test state_resolution_chain_100 ... bench:         153 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5656 ns/iter (+/- 5)

