# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 15:54:16 UTC
**Commit:** 1537398664d9507a0f4238307e9202a19b512145

## Results

test pagination_offset_deep_page ... bench:      126701 ns/iter (+/- 425)

test pagination_keyset_deep_page ... bench:          49 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         179 ns/iter (+/- 18)

test state_resolution_chain_100 ... bench:         197 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        8448 ns/iter (+/- 86)

