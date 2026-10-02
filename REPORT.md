# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 05:21:01 UTC
**Commit:** 91aa0ad373b19a4435d23635327bf82d74d172b5

## Results

test pagination_offset_deep_page ... bench:       71253 ns/iter (+/- 1397)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         277 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         273 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7195 ns/iter (+/- 19)

