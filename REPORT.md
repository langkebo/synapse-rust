# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 00:28:57 UTC
**Commit:** 707381f0a5277e3550db1eb1eee5d893fb738960

## Results

test pagination_offset_deep_page ... bench:       43039 ns/iter (+/- 79)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         220 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         225 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5972 ns/iter (+/- 19)

