# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-09 23:45:26 UTC
**Commit:** e2c8969aefb62afa1e7ebe348521ae4f8c9a5351

## Results

test pagination_offset_deep_page ... bench:       54178 ns/iter (+/- 309)

test pagination_keyset_deep_page ... bench:          55 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         279 ns/iter (+/- 9)

test state_resolution_chain_100 ... bench:         266 ns/iter (+/- 11)

test auth_chain_build_10 ... bench:        7425 ns/iter (+/- 77)

