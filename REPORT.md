# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 09:58:47 UTC
**Commit:** 2e9b1cc9ca4dd8a9560d359e273a67b3689d71f8

## Results

test pagination_offset_deep_page ... bench:      129363 ns/iter (+/- 148)

test pagination_keyset_deep_page ... bench:          61 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         263 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        9550 ns/iter (+/- 4)

