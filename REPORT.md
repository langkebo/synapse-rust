# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 02:28:46 UTC
**Commit:** 6c8dad4682ddae4eb0fb7e516fa67fa0f87c9b5c

## Results

test pagination_offset_deep_page ... bench:       42922 ns/iter (+/- 186)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         218 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         225 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5849 ns/iter (+/- 5)

