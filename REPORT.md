# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 09:39:31 UTC
**Commit:** c3d0bee850b1f998dca3778dfe2a990216a02f9d

## Results

test pagination_offset_deep_page ... bench:       70684 ns/iter (+/- 5303)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         286 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         280 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7228 ns/iter (+/- 8)

