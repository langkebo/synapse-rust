# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 03:03:59 UTC
**Commit:** 3af1ea0403c1692f72720e667f3a31cb1803b7e0

## Results

test pagination_offset_deep_page ... bench:       54296 ns/iter (+/- 40)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         275 ns/iter (+/- 4)

test auth_chain_build_10 ... bench:        6494 ns/iter (+/- 14)

