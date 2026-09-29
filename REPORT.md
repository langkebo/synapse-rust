# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 23:05:44 UTC
**Commit:** f1f61a1ee0b65209e8e062b87e53314b63ff0816

## Results

test pagination_offset_deep_page ... bench:       72059 ns/iter (+/- 2183)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 14)

test state_resolution_chain_100 ... bench:         276 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6341 ns/iter (+/- 69)

