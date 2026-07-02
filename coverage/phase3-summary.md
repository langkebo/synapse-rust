## Phase 3 Coverage Summary (2026-07-02)

**Overall**: 22.49% (3364/14960 lines) — up from 22.21% (+0.28pp)

### Per-module delta

| Module | Phase 2 | Phase 3 | Delta | Notes |
|--------|---------|---------|-------|-------|
| application_service.rs | 7.0% | 7.0% | — | Builder tests added |
| user.rs | 0.0% | 0.9% | +0.9pp | escape_like_pattern covered |
| search_index.rs | 8.7% | 8.7% | — | Cursor tests expanded |
| refresh_token/mod.rs | 0.0% | 7.7% | +7.7pp | Builder/struct tests |
| device/mod.rs | 47.4% | 47.4% | — | |
| worker.rs | 52.1% | 52.1% | — | |
| sliding_sync.rs | 11.7% | 11.7% | — | |
| room_summary.rs | 6.3% | 6.3% | — | |

### Tests added in Phase 3

| File | Tests | Type |
|------|-------|------|
| application_service.rs | 8 | Builder + struct serde |
| user.rs | 15 | escape_like_pattern + struct serde |
| search_index.rs | 10 | Cursor edge cases + structs |
| refresh_token/mod.rs | 11 | Builder + struct serde |
| **Total** | **49** | |

### Cumulative progress

| Phase | Tests Added | Coverage | Delta |
|-------|-------------|----------|-------|
| Baseline | — | ~10.5% | — |
| Phase 1 | 99 | 19.4% | +8.9pp |
| Phase 2 | 111 | 22.21% | +2.8pp |
| Phase 3 | 49 | 22.49% | +0.28pp |
| **Total** | **259** | **22.49%** | **+12pp** |

### Why Phase 3 delta is small

Struct field tests don't count toward line coverage — only executable code does. The remaining uncovered lines (>77%) are DB operations, async request handlers, and initialization code. Pure-logic testing surface is now largely saturated in synapse-storage.

### Path to 30%+

Requires DB-backed integration tests for the large storage modules:
- event/mod.rs: 510 uncovered lines
- room/mod.rs: 501 uncovered lines
- space.rs: 457 uncovered lines
- user.rs: 422 uncovered lines

These follow the pattern established in token.rs and device/mod.rs but require more complex foreign key setup.
