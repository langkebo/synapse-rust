# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-30 08:26:19 UTC
**Commit:** 1766de2a7ca9ff1099c0f018806820c00a3eba94

## Results

test pagination_offset_deep_page ... bench:       34708 ns/iter (+/- 586)

test pagination_keyset_deep_page ... bench:          30 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         156 ns/iter (+/- 4)

test state_resolution_chain_100 ... bench:         159 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        5605 ns/iter (+/- 86)

