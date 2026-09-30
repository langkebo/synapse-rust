# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 17:35:43 UTC
**Commit:** ae186d31a46fc45848b8bb5504001724b567bdb8

## Results

test pagination_offset_deep_page ... bench:       55511 ns/iter (+/- 116)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         292 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         289 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7452 ns/iter (+/- 56)

