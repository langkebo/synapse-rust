# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-28 11:26:10 UTC
**Commit:** ae8afd6fec8384001be0d59d2883d198325a887e

## Results

test pagination_offset_deep_page ... bench:       70278 ns/iter (+/- 248)

test pagination_keyset_deep_page ... bench:          55 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         287 ns/iter (+/- 17)

test state_resolution_chain_100 ... bench:         291 ns/iter (+/- 4)

test auth_chain_build_10 ... bench:        6302 ns/iter (+/- 9)

