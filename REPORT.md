# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-19 02:28:02 UTC
**Commit:** 12fb9c012c90f8551f4393233da44c38ea53457c

## Results

test pagination_offset_deep_page ... bench:       54969 ns/iter (+/- 157)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         272 ns/iter (+/- 20)

test state_resolution_chain_100 ... bench:         279 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7249 ns/iter (+/- 11)

