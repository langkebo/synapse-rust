# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 07:15:36 UTC
**Commit:** d525a5bc1d1efe42f74c2dd31650d80c232339c9

## Results

test pagination_offset_deep_page ... bench:       55725 ns/iter (+/- 328)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         293 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         308 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        7998 ns/iter (+/- 29)

