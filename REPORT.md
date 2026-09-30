# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 00:45:17 UTC
**Commit:** 0ac0a3456f6c4fa0f592c5d00f9b335b10d34df7

## Results

test pagination_offset_deep_page ... bench:       32312 ns/iter (+/- 682)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         149 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         156 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5628 ns/iter (+/- 5)

