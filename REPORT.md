# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-05 03:21:20 UTC
**Commit:** b55700db936630cac80ac9b6a6884fae66f0b7f5

## Results

test pagination_offset_deep_page ... bench:       54393 ns/iter (+/- 132)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         272 ns/iter (+/- 5)

test state_resolution_chain_100 ... bench:         282 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6747 ns/iter (+/- 9)

