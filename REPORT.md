# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 07:38:50 UTC
**Commit:** 416ece3c884ce706b6c74e52b3d7f69a591bd864

## Results

test pagination_offset_deep_page ... bench:       43135 ns/iter (+/- 64)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         216 ns/iter (+/- 0)

test state_resolution_chain_100 ... bench:         223 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5274 ns/iter (+/- 3)

