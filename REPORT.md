# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 14:00:46 UTC
**Commit:** 8869abea19e4881203ecf1d1b93c172ad6475ae2

## Results

test pagination_offset_deep_page ... bench:      140461 ns/iter (+/- 358)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         218 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         226 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        9948 ns/iter (+/- 24)

