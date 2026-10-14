# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 17:18:39 UTC
**Commit:** 3e007f8fbf00dad3686737b0ab3279a350d4b12a

## Results

test pagination_offset_deep_page ... bench:       74325 ns/iter (+/- 1529)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 8)

test state_resolution_chain_100 ... bench:         273 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        7201 ns/iter (+/- 8)

