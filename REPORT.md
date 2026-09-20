# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 12:22:08 UTC
**Commit:** 0bc8166e092dc83d8f18b4d509f4181d08d10054

## Results

test pagination_offset_deep_page ... bench:       43105 ns/iter (+/- 582)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         229 ns/iter (+/- 11)

test state_resolution_chain_100 ... bench:         228 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5954 ns/iter (+/- 35)

