# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 15:46:25 UTC
**Commit:** 95574e4c4904b1e91bf8e150aef8f79ac262f99f

## Results

test pagination_offset_deep_page ... bench:       69682 ns/iter (+/- 7494)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 25)

test state_resolution_chain_100 ... bench:         278 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6755 ns/iter (+/- 7)

