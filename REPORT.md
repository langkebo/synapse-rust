# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-21 01:47:15 UTC
**Commit:** 58d04e08a9a98bf94276490f765f00d4c3412adf

## Results

test pagination_offset_deep_page ... bench:       70594 ns/iter (+/- 760)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         274 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         283 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6640 ns/iter (+/- 13)

