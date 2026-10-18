# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-18 12:02:56 UTC
**Commit:** ff56f36384210e90aa04686ef6a161e5034f9e5c

## Results

test pagination_offset_deep_page ... bench:       54119 ns/iter (+/- 202)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 5)

test state_resolution_chain_100 ... bench:         276 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6659 ns/iter (+/- 48)

