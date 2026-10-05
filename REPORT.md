# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-05 05:11:32 UTC
**Commit:** 71ab09801c34023fe877d0f78bc0b04185c40261

## Results

test pagination_offset_deep_page ... bench:       55087 ns/iter (+/- 281)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         274 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        7376 ns/iter (+/- 41)

