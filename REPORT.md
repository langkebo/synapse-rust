# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-28 03:30:56 UTC
**Commit:** fbb941c014875f868f85327507bdcdf69f82ec5a

## Results

test pagination_offset_deep_page ... bench:       53303 ns/iter (+/- 343)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         309 ns/iter (+/- 15)

test state_resolution_chain_100 ... bench:         296 ns/iter (+/- 4)

test auth_chain_build_10 ... bench:        7455 ns/iter (+/- 22)

