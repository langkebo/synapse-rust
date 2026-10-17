# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-17 06:40:29 UTC
**Commit:** e0125f173e515416a52511c20cb9e3db50b6adb2

## Results

test pagination_offset_deep_page ... bench:       54421 ns/iter (+/- 367)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         271 ns/iter (+/- 24)

test state_resolution_chain_100 ... bench:         268 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6637 ns/iter (+/- 11)

