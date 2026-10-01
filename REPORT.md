# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 07:00:55 UTC
**Commit:** fd01650e09a9f39006e940121ec2a3dc931d62bf

## Results

test pagination_offset_deep_page ... bench:       70628 ns/iter (+/- 435)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         277 ns/iter (+/- 14)

test state_resolution_chain_100 ... bench:         276 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6343 ns/iter (+/- 37)

