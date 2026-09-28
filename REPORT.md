# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-28 17:00:59 UTC
**Commit:** c132515dc1b76b2eb383738c2b1a32f43cb260eb

## Results

test pagination_offset_deep_page ... bench:       55594 ns/iter (+/- 83)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         293 ns/iter (+/- 6)

test state_resolution_chain_100 ... bench:         323 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7310 ns/iter (+/- 20)

