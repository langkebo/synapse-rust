# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 07:38:17 UTC
**Commit:** cf845cb35caba8c3bdb56371310844b5a872f3fa

## Results

test pagination_offset_deep_page ... bench:       54208 ns/iter (+/- 137)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         282 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6335 ns/iter (+/- 30)

