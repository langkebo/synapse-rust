# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 07:18:41 UTC
**Commit:** f4d95c006936b3730ac2b17e3eb322f47ca9334a

## Results

test pagination_offset_deep_page ... bench:       42456 ns/iter (+/- 56)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         217 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         220 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5244 ns/iter (+/- 17)

