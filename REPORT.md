# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 06:46:56 UTC
**Commit:** 2695d5706fa44c9207df890fa975a6614cc34455

## Results

test pagination_offset_deep_page ... bench:       32734 ns/iter (+/- 283)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         149 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         157 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5688 ns/iter (+/- 29)

