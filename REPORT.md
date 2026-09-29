# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-29 03:09:15 UTC
**Commit:** fc3bd611da7d69d6bdab133d738b4f3486d783a1

## Results

test pagination_offset_deep_page ... bench:       71288 ns/iter (+/- 8687)

test pagination_keyset_deep_page ... bench:          54 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         260 ns/iter (+/- 25)

test state_resolution_chain_100 ... bench:         269 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6777 ns/iter (+/- 33)

