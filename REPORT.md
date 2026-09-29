# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 09:31:42 UTC
**Commit:** 86a170f93d0a64005e2c3705447ad196fb396935

## Results

test pagination_offset_deep_page ... bench:       54137 ns/iter (+/- 6723)

test pagination_keyset_deep_page ... bench:          55 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         272 ns/iter (+/- 5)

test state_resolution_chain_100 ... bench:         270 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6476 ns/iter (+/- 7)

