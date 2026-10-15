# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 07:52:42 UTC
**Commit:** 6302798c3582d67b01d27677c8ca9db00376b1c3

## Results

test pagination_offset_deep_page ... bench:      135512 ns/iter (+/- 1131)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         201 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         210 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        8942 ns/iter (+/- 156)

