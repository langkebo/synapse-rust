# Synapse Rust Performance Benchmark Report

**Date:** 2026-11-05 02:54:07 UTC
**Commit:** 7914e463009fd3ace79d3c6100ff609f10a68e1b

## Results

test pagination_offset_deep_page ... bench:       56364 ns/iter (+/- 265)

test pagination_keyset_deep_page ... bench:          55 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         290 ns/iter (+/- 16)

test state_resolution_chain_100 ... bench:         286 ns/iter (+/- 6)

test auth_chain_build_10 ... bench:        7017 ns/iter (+/- 9)

