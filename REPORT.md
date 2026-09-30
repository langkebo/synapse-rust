# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 08:26:13 UTC
**Commit:** 5c4e47d19b5f41bb5b84208953c8c49cc909992a

## Results

test pagination_offset_deep_page ... bench:       69762 ns/iter (+/- 1155)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         275 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         277 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7292 ns/iter (+/- 52)

