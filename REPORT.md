# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-22 11:13:26 UTC
**Commit:** c8e3bb904675203425029e460e0b0713a6e42fda

## Results

test pagination_offset_deep_page ... bench:       54202 ns/iter (+/- 355)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         269 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         268 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7107 ns/iter (+/- 9)

