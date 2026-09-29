# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 08:04:03 UTC
**Commit:** 20224d098364a91fc17881ad6351470dbfcc2907

## Results

test pagination_offset_deep_page ... bench:       54939 ns/iter (+/- 6565)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         265 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         270 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7533 ns/iter (+/- 18)

