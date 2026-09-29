# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 11:20:33 UTC
**Commit:** 47b794358dd0e8ff0bc3e8564e72c3fa25f56d3d

## Results

test pagination_offset_deep_page ... bench:       55482 ns/iter (+/- 69)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         296 ns/iter (+/- 24)

test state_resolution_chain_100 ... bench:         291 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        7674 ns/iter (+/- 11)

