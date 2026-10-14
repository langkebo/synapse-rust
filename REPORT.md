# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-14 10:29:58 UTC
**Commit:** 6263ce0c75fd4a82996c21fe34a421d7c7734cb7

## Results

test pagination_offset_deep_page ... bench:       34494 ns/iter (+/- 759)

test pagination_keyset_deep_page ... bench:          31 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         150 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         164 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        5893 ns/iter (+/- 168)

