# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-21 05:52:05 UTC
**Commit:** 5597f8d2e990940c052b12d5155875586ded8a2f

## Results

test pagination_offset_deep_page ... bench:       54196 ns/iter (+/- 328)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 15)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 6)

test auth_chain_build_10 ... bench:        7505 ns/iter (+/- 10)

