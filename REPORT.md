# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-19 05:28:29 UTC
**Commit:** b2285c379db8121191433abc39f05d9cbdcf3a20

## Results

test pagination_offset_deep_page ... bench:       34521 ns/iter (+/- 387)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         146 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         157 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        5709 ns/iter (+/- 34)

