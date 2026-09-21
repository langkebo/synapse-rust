# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-21 04:32:58 UTC
**Commit:** f47898b47d5663b245c7c96222315a9bb232d31a

## Results

test pagination_offset_deep_page ... bench:       42874 ns/iter (+/- 103)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         227 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         224 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5901 ns/iter (+/- 12)

