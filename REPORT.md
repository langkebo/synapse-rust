# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 12:19:05 UTC
**Commit:** 57fe25445755683e3d8b49307e9c391e67fd4164

## Results

test pagination_offset_deep_page ... bench:       55339 ns/iter (+/- 76)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         293 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         284 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7879 ns/iter (+/- 37)

