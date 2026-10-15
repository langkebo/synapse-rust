# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 07:40:35 UTC
**Commit:** 326d3fd451b06d824d604a13c1aa8efd6202b510

## Results

test pagination_offset_deep_page ... bench:       43097 ns/iter (+/- 88)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         209 ns/iter (+/- 6)

test state_resolution_chain_100 ... bench:         212 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5532 ns/iter (+/- 6)

