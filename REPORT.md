# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 15:26:49 UTC
**Commit:** 973d0ce69408e353e53103d427ec22ec68bca69a

## Results

test pagination_offset_deep_page ... bench:       56066 ns/iter (+/- 7384)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         269 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7286 ns/iter (+/- 7)

