# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 06:48:15 UTC
**Commit:** 6908bd4fa196ec909f83295f88cdce5f9f362fa2

## Results

test pagination_offset_deep_page ... bench:       54242 ns/iter (+/- 8963)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         268 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         277 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7023 ns/iter (+/- 5)

