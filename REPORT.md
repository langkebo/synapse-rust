# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-21 09:46:48 UTC
**Commit:** 9c374ce13e9ec909b4cfd1a8926f0758d5b47dbc

## Results

test pagination_offset_deep_page ... bench:       70303 ns/iter (+/- 66)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         273 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         283 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7660 ns/iter (+/- 20)

