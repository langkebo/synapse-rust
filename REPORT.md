# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-15 14:49:30 UTC
**Commit:** 67998d92fada2a469b9cf1241f19b6394d96ed6b

## Results

test pagination_offset_deep_page ... bench:       54188 ns/iter (+/- 4378)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 22)

test state_resolution_chain_100 ... bench:         281 ns/iter (+/- 3)

test auth_chain_build_10 ... bench:        6595 ns/iter (+/- 17)

