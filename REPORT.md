# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 07:12:31 UTC
**Commit:** a01039a6ce7faa7459aa176c3bfb4bd074a5135b

## Results

test pagination_offset_deep_page ... bench:       32576 ns/iter (+/- 751)

test pagination_keyset_deep_page ... bench:          29 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         152 ns/iter (+/- 7)

test state_resolution_chain_100 ... bench:         161 ns/iter (+/- 4)

test auth_chain_build_10 ... bench:        5801 ns/iter (+/- 199)

