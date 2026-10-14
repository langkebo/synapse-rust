# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 04:53:34 UTC
**Commit:** cc56d479e75e46e21c004419403abd8f1e913fc2

## Results

test pagination_offset_deep_page ... bench:       54433 ns/iter (+/- 94)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 15)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6533 ns/iter (+/- 19)

