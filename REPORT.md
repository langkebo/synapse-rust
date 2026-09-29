# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 11:45:47 UTC
**Commit:** 92e314f4a56ee60f6e55c4a938cc51b14b3aad75

## Results

test pagination_offset_deep_page ... bench:       54275 ns/iter (+/- 82)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 24)

test state_resolution_chain_100 ... bench:         287 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7027 ns/iter (+/- 94)

