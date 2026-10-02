# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-02 02:16:53 UTC
**Commit:** 94bbdb268228be987c296a972f26c7af1dd3ae68

## Results

test pagination_offset_deep_page ... bench:       69973 ns/iter (+/- 3269)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         270 ns/iter (+/- 2)

test state_resolution_chain_100 ... bench:         283 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6545 ns/iter (+/- 7)

