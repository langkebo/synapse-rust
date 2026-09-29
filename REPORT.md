# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 03:41:17 UTC
**Commit:** 6663791918c1ffcffa981b0f9764854e0685492d

## Results

test pagination_offset_deep_page ... bench:      145602 ns/iter (+/- 2330)

test pagination_keyset_deep_page ... bench:          59 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         210 ns/iter (+/- 15)

test state_resolution_chain_100 ... bench:         230 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:       10200 ns/iter (+/- 9)

