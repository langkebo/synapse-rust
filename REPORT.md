# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 06:54:16 UTC
**Commit:** 8cf03f9d079e7f5093b903ab80c5301a8cb7dc26

## Results

test pagination_offset_deep_page ... bench:       54086 ns/iter (+/- 373)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         281 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6595 ns/iter (+/- 109)

