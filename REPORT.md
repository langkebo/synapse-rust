# Synapse Rust Performance Benchmark Report

**Date:** 2026-09-20 13:38:17 UTC
**Commit:** b0630157da9b1e8263ed185a55392ef95e99684b

## Results

test pagination_offset_deep_page ... bench:       33462 ns/iter (+/- 435)

test pagination_keyset_deep_page ... bench:          30 ns/iter (+/- 0)

test state_resolution_chain_10 ... bench:         156 ns/iter (+/- 3)

test state_resolution_chain_100 ... bench:         166 ns/iter (+/- 2)

test auth_chain_build_10 ... bench:        6013 ns/iter (+/- 95)

