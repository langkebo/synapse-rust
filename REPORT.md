# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 02:07:25 UTC
**Commit:** c6919e3305373459af782539d9720165e60928e7

## Results

test pagination_offset_deep_page ... bench:       54770 ns/iter (+/- 782)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         274 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         281 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6553 ns/iter (+/- 19)

