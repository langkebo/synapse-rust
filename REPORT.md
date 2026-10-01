# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 23:12:44 UTC
**Commit:** 8ce2d3f47f8bf1ed4a27cf78668a0a383f08393b

## Results

test pagination_offset_deep_page ... bench:       36455 ns/iter (+/- 348)

test pagination_keyset_deep_page ... bench:          31 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         159 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         166 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6022 ns/iter (+/- 96)

