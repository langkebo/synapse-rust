# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-13 10:47:11 UTC
**Commit:** ed09854e55c4459f0128c555c2b689c8c7180b56

## Results

test pagination_offset_deep_page ... bench:       54580 ns/iter (+/- 52)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         268 ns/iter (+/- 10)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6731 ns/iter (+/- 39)

