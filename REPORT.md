# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 14:48:37 UTC
**Commit:** 1b7fb2251431f6e3eb744f15498a201849452d41

## Results

test pagination_offset_deep_page ... bench:       32997 ns/iter (+/- 649)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         158 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         165 ns/iter (+/- 5)

test auth_chain_build_10 ... bench:        5742 ns/iter (+/- 131)

