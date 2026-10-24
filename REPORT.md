# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-24 00:32:08 UTC
**Commit:** 79b9d2de5d170045e0254c71876ec6ce420776c6

## Results

test pagination_offset_deep_page ... bench:       31095 ns/iter (+/- 195)

test pagination_keyset_deep_page ... bench:          32 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         145 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         154 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        5599 ns/iter (+/- 25)

