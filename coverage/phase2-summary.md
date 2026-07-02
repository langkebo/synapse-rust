## Phase 2 Coverage Summary (2026-07-02)

**Overall**: 22.21% (3322/14960 lines) — up from 19.4% (+2.8pp)

### Per-module delta

| Module | Phase 1 | Phase 2 | Delta | Lines |
|--------|---------|---------|-------|-------|
| crypto.rs | 68.1% | 68.1% | — | 113/166 |
| error.rs | 61.4% | 61.4% | — | 259/422 |
| time.rs | 100.0% | 100.0% | — | 14/14 |
| token.rs | 74.1% | 74.1% | — | 80/108 |
| moderation.rs | 58.9% | 58.9% | — | 53/90 |
| **worker.rs** | 52.1% | 52.1% | — | 275/528 |
| **sliding_sync.rs** | 11.7% | 11.7% | — | 42/360 |
| **room_summary.rs** | 56.7% (17/30) | 6.3% (20/318) | mixed* | 20/318 |
| **device/mod.rs** | 18.2% | **47.4%** | **+29.2pp** | 172/363 |

*room_summary.rs: Phase 1 measured a 30-line file; Phase 2 measured the 318-line storage file.

### Tests added in Phase 2

| Task | File | Tests | Type |
|------|------|-------|------|
| 8 | worker.rs | 65 | Pure unit (enum/struct) |
| 9 | sliding_sync.rs | 16 | Pure unit (filter SQL) |
| 10 | room_summary.rs | 5 | Pure unit (conversion) |
| 11 | device/mod.rs | 25 | DB-backed integration |
| **Total** | | **111** | |

### Cumulative test count (Phase 1+2)

- Phase 1: 99 tests (crypto 8 + error 69 + time 4 + token 12 + moderation 6)
- Phase 2: 111 tests (worker 65 + sliding_sync 16 + room_summary 5 + device 25)
- **Total: 210 tests**

### Phase 3 targets (largest files <30%)

| File | Lines | Coverage | Priority |
|------|-------|----------|----------|
| event/mod.rs | 510 | 0.0% | High — pure SQL mapping |
| room/mod.rs | 501 | 0.0% | High — core domain |
| user.rs | 426 | 0.0% | High — trigram search SQL |
| sliding_sync.rs | 360 | 11.7% | Medium — already partial |
| application_service.rs | 330 | 0.0% | Medium — namespace conflict |
| membership/mod.rs | 317 | 0.0% | Medium — FK-heavy |
| room_summary.rs | 318 | 6.3% | Medium — already partial |
| presence/mod.rs | 269 | 0.0% | Low — FK-heavy |
| thread.rs | 249 | 0.0% | Low |
| push_notification.rs | 214 | 10.3% | Low |
