# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 08:00:24 UTC
**Commit:** 6dbc1bad61f86eaa7c12fc57a7550fb94046e3f7

## Results

test pagination_offset_deep_page ... bench:      131901 ns/iter (+/- 116)

test pagination_keyset_deep_page ... bench:          49 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         180 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         194 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        8461 ns/iter (+/- 33)

