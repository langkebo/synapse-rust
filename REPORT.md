# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 08:07:20 UTC
**Commit:** 2babd6011c068fef3a333379da26349b2e6f1fbe

## Results

test pagination_offset_deep_page ... bench:       70724 ns/iter (+/- 287)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         265 ns/iter (+/- 12)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 5)

test auth_chain_build_10 ... bench:        6679 ns/iter (+/- 50)

