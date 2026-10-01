# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 04:30:00 UTC
**Commit:** 9c83ad0ffbf0e0a9968bb03c5d16a384173ca191

## Results

test pagination_offset_deep_page ... bench:       55561 ns/iter (+/- 112)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         293 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         285 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        7358 ns/iter (+/- 13)

