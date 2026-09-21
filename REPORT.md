# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-21 02:15:15 UTC
**Commit:** 712805507a030e6b3435ad003668bb5d16b308b9

## Results

test pagination_offset_deep_page ... bench:       35882 ns/iter (+/- 359)

test pagination_keyset_deep_page ... bench:          30 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         157 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         165 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        6021 ns/iter (+/- 130)

