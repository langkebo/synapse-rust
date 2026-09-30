# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 05:51:52 UTC
**Commit:** 85bfd51acc9bcaf94cc38166b6887e81ed2c191b

## Results

test pagination_offset_deep_page ... bench:      120792 ns/iter (+/- 341)

test pagination_keyset_deep_page ... bench:          50 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         184 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         199 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        8641 ns/iter (+/- 37)

