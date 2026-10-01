# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 00:40:51 UTC
**Commit:** c65541635381c660e57d4788e0f784dc163f828d

## Results

test pagination_offset_deep_page ... bench:      139821 ns/iter (+/- 74)

test pagination_keyset_deep_page ... bench:          57 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         209 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         225 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:       12237 ns/iter (+/- 16)

