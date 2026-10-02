# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 06:31:20 UTC
**Commit:** 8afbabd28467be7d3e77c8474b5ed259c2aa6206

## Results

test pagination_offset_deep_page ... bench:       54200 ns/iter (+/- 90)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         273 ns/iter (+/- 5)

test state_resolution_chain_100 ... bench:         274 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6985 ns/iter (+/- 15)

