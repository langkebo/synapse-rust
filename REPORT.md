# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 10:34:21 UTC
**Commit:** 50cba8c28322e08856923857fb262263a6b8a8c0

## Results

test pagination_offset_deep_page ... bench:       54910 ns/iter (+/- 47)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         265 ns/iter (+/- 11)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7466 ns/iter (+/- 7)

