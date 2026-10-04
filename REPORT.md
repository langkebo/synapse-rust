# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-04 01:11:21 UTC
**Commit:** 2609889afefac6eafb0527cf2540523ca66a75ea

## Results

test pagination_offset_deep_page ... bench:       70113 ns/iter (+/- 95)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         283 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        6488 ns/iter (+/- 14)

