# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 01:51:28 UTC
**Commit:** 5aa9775273c0fc277908702e3ce4322e6bfb83d8

## Results

test pagination_offset_deep_page ... bench:       55331 ns/iter (+/- 48)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         300 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         298 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        9878 ns/iter (+/- 8)

