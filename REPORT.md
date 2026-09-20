# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 07:12:44 UTC
**Commit:** 88ac3dcb06d45fa8f85168bab55cc7708b27db06

## Results

test pagination_offset_deep_page ... bench:       51499 ns/iter (+/- 296)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         269 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         266 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6640 ns/iter (+/- 48)

