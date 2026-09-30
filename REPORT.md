# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 01:56:41 UTC
**Commit:** 8aade602c344e5e94fa113c49dedaa04e1e15bec

## Results

test pagination_offset_deep_page ... bench:       43396 ns/iter (+/- 81)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         218 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         220 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5930 ns/iter (+/- 41)

