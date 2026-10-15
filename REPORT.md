# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 04:27:50 UTC
**Commit:** b9145cd5041128634e83111495b0971af7f8a070

## Results

test pagination_offset_deep_page ... bench:       55245 ns/iter (+/- 60)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         276 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7856 ns/iter (+/- 9)

