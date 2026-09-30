# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 00:09:28 UTC
**Commit:** e89201bb04aec71a81a93d5de7dd55ffa6632e41

## Results

test pagination_offset_deep_page ... bench:       54441 ns/iter (+/- 80)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         276 ns/iter (+/- 14)

test state_resolution_chain_100 ... bench:         277 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6646 ns/iter (+/- 56)

