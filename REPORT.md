# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-28 18:22:05 UTC
**Commit:** 9e8661b1abd7c5f70d5b04100cef26359fef3b7f

## Results

test pagination_offset_deep_page ... bench:       56832 ns/iter (+/- 160)

test pagination_keyset_deep_page ... bench:          55 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         291 ns/iter (+/- 16)

test state_resolution_chain_100 ... bench:         292 ns/iter (+/- 4)

test auth_chain_build_10 ... bench:        6445 ns/iter (+/- 56)

