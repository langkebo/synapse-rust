# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 08:55:06 UTC
**Commit:** 1d728dbe09b3e617b7049f83b54cdb3d7ae8e12e

## Results

test pagination_offset_deep_page ... bench:       54712 ns/iter (+/- 2206)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         281 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6806 ns/iter (+/- 16)

