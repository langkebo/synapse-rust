# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-09 13:25:57 UTC
**Commit:** 09074226611c8efb648052e9ae0a0429ad823301

## Results

test pagination_offset_deep_page ... bench:       33504 ns/iter (+/- 441)

test pagination_keyset_deep_page ... bench:          32 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         146 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         155 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        5863 ns/iter (+/- 98)

