# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 05:35:47 UTC
**Commit:** 0d7210a5225b46d5534bcab074e88ebde264b183

## Results

test pagination_offset_deep_page ... bench:       34177 ns/iter (+/- 383)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         147 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         166 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        5649 ns/iter (+/- 28)

