# Synapse Rust Performance Benchmark Report

**Date:** 2026-10-01 07:48:30 UTC
**Commit:** 64a015a8d5259718d8039edd6d365fc35a98d540

## Results

test pagination_offset_deep_page ... bench:       43027 ns/iter (+/- 25)

test pagination_keyset_deep_page ... bench:          44 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         240 ns/iter (+/- 1)

test state_resolution_chain_100 ... bench:         230 ns/iter (+/- 1)

test auth_chain_build_10 ... bench:        6164 ns/iter (+/- 16)

