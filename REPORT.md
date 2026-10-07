# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-07 08:51:02 UTC
**Commit:** 3699d03554d9cf8fcbb0618e3bf4883f693196bf

## Results

test pagination_offset_deep_page ... bench:       54155 ns/iter (+/- 236)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         268 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7065 ns/iter (+/- 122)

