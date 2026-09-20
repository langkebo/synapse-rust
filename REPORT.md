# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 12:27:08 UTC
**Commit:** 0e289e25abc9c8c93979b7b1ce93c4ba9471be5d

## Results

test pagination_offset_deep_page ... bench:       54695 ns/iter (+/- 83)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         268 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         273 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6798 ns/iter (+/- 66)

