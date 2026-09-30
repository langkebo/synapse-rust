# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 12:04:50 UTC
**Commit:** 242db0476668e854d6846cf379f458ac541bc7a8

## Results

test pagination_offset_deep_page ... bench:       55786 ns/iter (+/- 59)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         302 ns/iter (+/- 17)

test state_resolution_chain_100 ... bench:         304 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7548 ns/iter (+/- 13)

