# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 06:29:28 UTC
**Commit:** c6a2d0cd81a1e6dded9c74e4877896bda7894aa8

## Results

test pagination_offset_deep_page ... bench:       36749 ns/iter (+/- 396)

test pagination_keyset_deep_page ... bench:          31 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         158 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         169 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6180 ns/iter (+/- 62)

