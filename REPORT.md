# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-21 11:16:48 UTC
**Commit:** dccae34f8758a91a1c053a7bb5b3a6a28296766b

## Results

test pagination_offset_deep_page ... bench:       56211 ns/iter (+/- 54)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 0)

test state_resolution_chain_100 ... bench:         284 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7124 ns/iter (+/- 11)

