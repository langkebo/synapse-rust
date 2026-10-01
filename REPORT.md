# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 09:23:05 UTC
**Commit:** 7c4b1c88460d050277bff21eb27368c28c4c93eb

## Results

test pagination_offset_deep_page ... bench:       55083 ns/iter (+/- 300)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         275 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         279 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6933 ns/iter (+/- 23)

