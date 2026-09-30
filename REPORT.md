# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 02:05:09 UTC
**Commit:** 362c430d269c3d4c6bbfc64d24603d1e1a65faf2

## Results

test pagination_offset_deep_page ... bench:      127580 ns/iter (+/- 10706)

test pagination_keyset_deep_page ... bench:          49 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         186 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         196 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        8615 ns/iter (+/- 55)

