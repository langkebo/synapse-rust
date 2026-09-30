# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 04:30:10 UTC
**Commit:** c85ebd4e25fb6750c7f665b125c1e2ac534c6988

## Results

test pagination_offset_deep_page ... bench:      140494 ns/iter (+/- 214)

test pagination_keyset_deep_page ... bench:          43 ns/iter (+/- 1)

test state_resolution_chain_10 ... bench:         153 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         161 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        6540 ns/iter (+/- 157)

