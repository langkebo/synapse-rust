# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 07:32:03 UTC
**Commit:** eefcd88aa67c72779a917d623c3ba59a0cd357c2

## Results

test pagination_offset_deep_page ... bench:       54067 ns/iter (+/- 7236)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         278 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         285 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6430 ns/iter (+/- 21)

