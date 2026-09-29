# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 12:36:59 UTC
**Commit:** 7ff7a13b799254af341b4580fcce55be8240d55d

## Results

test pagination_offset_deep_page ... bench:       42574 ns/iter (+/- 175)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         218 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         235 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        5278 ns/iter (+/- 61)

