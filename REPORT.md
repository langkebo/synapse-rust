# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 01:20:04 UTC
**Commit:** d59be5be58e2fe7a06c020a9d76774989155a4e6

## Results

test pagination_offset_deep_page ... bench:       55570 ns/iter (+/- 185)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         290 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         292 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6802 ns/iter (+/- 9)

