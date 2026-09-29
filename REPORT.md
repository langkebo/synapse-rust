# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 12:03:26 UTC
**Commit:** 1744d306f122ea5abb4171b40d28becbd84dd594

## Results

test pagination_offset_deep_page ... bench:       55730 ns/iter (+/- 320)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         293 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         293 ns/iter (+/- 12)

test auth_chain_build_10 ... bench:        7488 ns/iter (+/- 8)

