# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 14:06:27 UTC
**Commit:** 74bb9c522bdeefcd0b816def2247ba73b4fcd1a2

## Results

test pagination_offset_deep_page ... bench:       31217 ns/iter (+/- 105)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         145 ns/iter (+/- 0)

test state_resolution_chain_100 ... bench:         158 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5659 ns/iter (+/- 23)

