# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 06:02:45 UTC
**Commit:** 6e487ddb2555304948b7db3ef63e37c13ec40af8

## Results

test pagination_offset_deep_page ... bench:      140799 ns/iter (+/- 269)

test pagination_keyset_deep_page ... bench:          42 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         152 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         166 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6563 ns/iter (+/- 73)

