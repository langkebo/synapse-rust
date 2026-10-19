# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-19 06:54:36 UTC
**Commit:** eb8f40b7ac5941a20cac1ce0eb9180004b4e5e5c

## Results

test pagination_offset_deep_page ... bench:      126506 ns/iter (+/- 394)

test pagination_keyset_deep_page ... bench:          61 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         257 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         262 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        9166 ns/iter (+/- 9)

