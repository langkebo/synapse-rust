# 数据库架构优化计划 — Tracer Bullet Tickets

**Source spec:** `artifacts/数据库架构诊断报告-2026-08-30.md`
**Tracker:** local markdown (`.scratch/db-schema-optimization/`)
**Created:** 2026-08-30
**Tickets:** 5 (all unblocked, can run in parallel or sequentially)

---

## Tickets overview

| **DB-01** | P0-4 重复索引 | ✅ Done: 73 dups removed (commit a00ac4a1) | ✅ Done |
| **DB-01-b** | ~~P0-5 schema_migrations~~ → Not redundant (used by schema_validator) | Skipped | — |
| **DB-04** | P0-6 default admin hardcoded | ✅ Done: moved to scripts/create-default-admin.sql (commit d81043f1) | ✅ Done |
| **DB-04-b** | P0-3 events CASCADE FK | ⏸ Deferred (requires delete_room Rust refactor) | Skipped |
| 02 | Expand schema health check coverage | P0-1 | None | 🔴 P0 |
| 03 | Fix event txn boundary + dedup race | P1-3 | None | 🟠 P1 |
| 04 | Remove events CASCADE + fix default admin | P0-3, P0-6 | None | 🔴 P0 |
| 05 | Add missing event_relations indexes | P1-1 | None | 🟠 P1 |

All tickets are **independent** — none block each other. You can claim them in any order, run in parallel, or pick one to focus on first.

---

## Out of scope (deferred to future efforts)

These items from the diagnostic report are **not** included in this batch of tickets because they require deeper architectural changes:

- **P0-2** (room_memberships / room_summary_members data drift): Requires cross-table migration plan, multi-PR coordination
- **P1-2** (rooms_summaries_mv LATERAL N+1): Requires materialized view rewrite, performance benchmarking
- **P1-4** (DO$$ block constraint cleanup): Mechanical but spread across 60+ blocks, low priority
- **P1-5** (connection pool worker-aware): Configuration-level change, not a migration
- **P1-6** (materialized view refresh strategy): Configuration + cron change
- **P1-7** (migration file naming): Cosmetic
- **P2-1** through **P2-4**: Code quality / maintenance, can be addressed in incremental lint passes

These belong in a separate `db-schema-optimization-phase-2` effort when this batch lands.

---

## Verification checklist (after all tickets complete)

- [ ] `cargo check --all-features` — clean
- [ ] `cargo test --package synapse-storage --lib` — all pass
- [ ] `cargo sqlx migrate run` — succeeds with no errors
- [ ] Fresh database has no duplicate indexes (verify with `pg_indexes` query)
- [ ] `schema_health_check` returns PASS on fresh database
- [ ] Default admin account not present in schema; deploy script works
- [ ] `event_relations` has new GIN + covering indexes
- [ ] Event writes succeed with explicit transaction

---

## Work the frontier

These tickets are all on the **frontier**: no blockers. Pick any one and claim it.

Recommended execution order (by risk-avoidance, not priority):

1. **DB-05** (lowest risk, additive only) — get a quick win
2. **DB-01** (read-only refactor of baseline file) — mechanical, well-scoped
3. **DB-04** (additive migration) — straightforward migration
4. **DB-02** (Rust code change) — requires deeper refactor but no DB migration
5. **DB-03** (touches hot path) — needs care, do last after patterns are established

After each ticket lands, re-run the full verification checklist.