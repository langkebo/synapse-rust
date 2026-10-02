# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 02:50:33 UTC
**Commit:** fdad5c467c6f3220973b9ab649828f1e15e6bdb5

## Results

test pagination_offset_deep_page ... bench:      126667 ns/iter (+/- 337)

test pagination_keyset_deep_page ... bench:          61 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         264 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         268 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        9408 ns/iter (+/- 7)

