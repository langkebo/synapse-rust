# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 13:52:50 UTC
**Commit:** 3439a0ebdc4d391351ea1584fc70c3b77928961d

## Results

test pagination_offset_deep_page ... bench:       70501 ns/iter (+/- 407)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         284 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         278 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7299 ns/iter (+/- 25)

