# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 08:33:14 UTC
**Commit:** d853027ca9b8e33f5e886ea79b7ae7e8c2b47618

## Results

test pagination_offset_deep_page ... bench:       70194 ns/iter (+/- 886)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         285 ns/iter (+/- 5)

test state_resolution_chain_100 ... bench:         270 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6904 ns/iter (+/- 29)

