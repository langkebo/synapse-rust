# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 09:00:28 UTC
**Commit:** 5b9540d806502edbee99a4de7e5cbcea6c35e4c8

## Results

test pagination_offset_deep_page ... bench:       54848 ns/iter (+/- 82)

test pagination_keyset_deep_page ... bench:          53 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         266 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         272 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6982 ns/iter (+/- 12)

