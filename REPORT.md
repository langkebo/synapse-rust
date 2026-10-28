# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-28 06:56:28 UTC
**Commit:** ffc9c79bd23e496f285bcb58ca217c890d38be39

## Results

test pagination_offset_deep_page ... bench:       41377 ns/iter (+/- 47)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         236 ns/iter (+/- 14)

test state_resolution_chain_100 ... bench:         233 ns/iter (+/- 8)

test auth_chain_build_10 ... bench:        4878 ns/iter (+/- 11)

