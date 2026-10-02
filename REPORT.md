# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 09:54:57 UTC
**Commit:** f8c45b73d2f371fe8e5eadf613ade6a44a50ec9a

## Results

test pagination_offset_deep_page ... bench:       54904 ns/iter (+/- 60)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         275 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         278 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        7240 ns/iter (+/- 7)

