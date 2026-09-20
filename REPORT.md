# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 10:30:58 UTC
**Commit:** d204e043c4919ed4775a894ff21280a3c2569a4a

## Results

test pagination_offset_deep_page ... bench:       70161 ns/iter (+/- 608)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         267 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         274 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6613 ns/iter (+/- 24)

