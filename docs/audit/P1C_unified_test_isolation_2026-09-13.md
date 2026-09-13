# P1-C — Unified test isolation: full regression and determinism record

- Date: 2026-09-13
- Branch: `perf/unify-test-isolation`
- Base commit: `de3df1fb`
- HEAD when this record was written: `e4823a8b`
- Worktree: `/Users/ljf/Desktop/hu_ts/synapse-rust-wt-unify`
- Database: **only** `synapse_test` at `192.168.107.2:5432`; `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE` unset for every command
- Build env: `SQLX_OFFLINE=true`, `CARGO_TARGET_DIR=/tmp/wt_unify_target`, `--test-threads 4`

This document closes the plan `docs/superpowers/plans/2026-09-13-unify-test-isolation.md`.
Every number below is either the output of a command run for this record or is explicitly
attributed to the earlier per-task evidence listed in §7. Nothing here is projected.

---

## 1. What was unified and why

Two crates had grown separate schema-per-test fixtures that had drifted apart.

1. **`synapse-storage::test_isolation::IsolatedTestPool`** replayed
   `migrations/00000000_unified_schema_v11.sql` statement-by-statement on **every**
   `new()` call. Instrumented before-measurement (P1-B §2, 89-case queue):
   `baseline_replay` median **4.416s**, p90 5.093s, max 5.815s,
   Σ 393.5s — **99% of fixture cost** (`setup` median 4.459s).

2. **`synapse-services::test_utils::prepare_isolated_test_pool`** created an **empty**
   schema and let the runtime `DatabaseInitService` fill it. That initializer does not
   create every baseline table — notably the retention tables
   (`grep -rn retention synapse-services/src/database_initializer/` has no hits). With
   `search_path = <schema>, public` a missing table **silently resolved to the shared
   `public` schema**, so the retention cohort (`retention_service::db_tests`) mutated one
   shared `server_retention_policy` row and drifted: 4 of 5 runs red at
   `--test-threads 4`, with the failing test moving between runs.

Why it mattered: the `--workspace --lib` gate was probabilistically red on one commit and
one set of parameters — `6128/0`, `6116/4`, `6117/3` (P1-B §1) — with failures reported as
`Operation timed out`. The storage fixture's 4.4 s/test replay saturated the 1.5-CPU
Postgres container; `--test-threads 8` produced server-side
`FATAL: canceling authentication due to timeout`, and `--threads 4` was both faster and
stable. The shared module gives both crates one fixture: build the baseline into a cached
template once, then clone it per test.

Out of scope for this plan (not touched, recorded here so the boundary is explicit): the
CI-configuration defect where the test job points `TEST_DATABASE_URL` at the application
database **and** sets `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE=1`. One local run in that
configuration wiped `public` (253 → 3 tables, `schema_migrations` 37 → 0) and produced
1033 false failures (P1-B §5b).

---

## 2. The shared design — `synapse-common/src/test_isolation.rs`

| Element | Implementation |
|---|---|
| Template name | `template_schema_name()` = `test_isolation_template_` + FNV-1a 64-bit hex of the baseline SQL |
| Fingerprint | `baseline_fingerprint()`. For the workspace baseline `concat!(00000000_unified_schema_v11.sql, 00000001_extensions_v10.sql)` — **v11 then extensions, with NO separator** — the fingerprint is **`bec240fb79ed438b`** |
| Build-once serialization | `ensure_template_schema()` takes a session-scoped `pg_advisory_lock` (key `0x5359_4E41_5053_5445`), released on success and on failure |
| Readiness marker | `_synapse_test_template_ready`, written into the template only after all baseline statements succeed; a half-built template (timeout/SIGKILL) is detected as incomplete and rebuilt |
| Clone | `clone_schema_from_template(pool, schema, template)` runs one `DO $do$` round trip (see below) |
| Validation | `validate_clone()` compares clone vs template on tables, foreign keys, functions, views, matviews and triggers, and errors with **both** counts |

Content fingerprint details that are load-bearing: the fingerprint covers the *exact bytes* of
the concatenated baseline, so concat order and any separator change the template identity. A
separator yields `a05fa4488475fe1d`; reversed order yields `4137af770181767b` (recorded as a
cross-task constraint in the SDD ledger). Both would silently mint a second template rather
than reuse the intended one.

The clone is a single `DO $do$` block with three steps:

- **Phase 1** — per baseline table: `CREATE TABLE <clone>.<t> (LIKE <template>.<t> INCLUDING ALL)`
  (columns, defaults, identity, indexes, PK/UNIQUE/CHECK).
- **Phase 1b** — per baseline table: `INSERT INTO <clone>.<t> SELECT * FROM <template>.<t>`
  (see §4). Positional `SELECT *` is sound because `LIKE` preserves column order.
- **Phase 2** — switch `search_path` to the clone (preserving the caller's tail) and replay
  **functions, views/materialized views, foreign keys and triggers**. PL/pgSQL bodies are not
  schema-bound, so replaying them with the template on `search_path` would bind unqualified
  names to the template's tables; view and trigger definitions are stripped of the template
  qualifier and triggers are re-pointed with
  `EXECUTE FUNCTION <template>.` → `EXECUTE FUNCTION <clone>.`.

The template's readiness marker is excluded from both the clone and the validator's table
count: it is fixture bookkeeping, not baseline inventory.

Measured inventory of the live `v11 ++ extensions` template
`test_isolation_template_bec240fb79ed438b` (queried directly for this record):

```
tables=254  fks=127  functions=6  views=2  matviews=2  triggers=1
seed rows: sync_stream_id=4  server_retention_policy=1  server_media_quota=1
public tables (not wiped): 253
```

The clone drops the marker, so an isolated clone schema carries the **253 baseline tables**
(not the retired services fixture's partial schema). The plan's stated expectation of
"254 rather than 9" is satisfied in the sense that the shared template holds the full
254-object table inventory (253 baseline + marker) and both consumers now clone from it; the
retired empty-schema path is not re-run in this record, so its exact table count is reported
here as the plan's expectation, not as a measurement made by Task 7.

---

## 3. Three defects a green full-suite run had missed

The template-clone implementation first landed on the sibling branch
`perf/test-isolation-template` (commits `b3174dee`, `cd13acaf`). That branch ran the **full
suite green** while carrying three real defects. The shared extraction inherited them
verbatim; they were caught in the Task 3 review and fixed in `67fd21cc`. Stated plainly:

1. **View/materialized-view creation order was inverted.** Views were created before the
   matviews they read. Where `public` happened to lack the referenced matview the clone
   failed with `relation "unify_inner_mv" does not exist`; on the real baseline it failed
   *silently* by binding the stripped, unqualified reference to `public`. Measured on a real
   clone of `test_isolation_template_bec240fb79ed438b`: before the fix,
   `public_room_directory`'s **13** references pointed at `public.rooms_summaries_mv`; after
   ordering by depth descending, **0** did (all 13 point at the clone's matview).
2. **Trigger definitions were left template-qualified.** `pg_get_triggerdef` renders
   `EXECUTE FUNCTION <template>.fn()`; only the `ON` table was re-pointed. Every clone
   therefore held a real dependency on the template schema, so
   `ensure_template_schema`'s `DROP SCHEMA ... CASCADE` would cascade into every clone's
   triggers. Runtime execution still resolved through `search_path`, which is why the
   full-suite run stayed green.
3. **`validate_clone` could pass vacuously on a missing schema.** Its inventory was built
   from the *requested* schema names rather than the catalog, so a nonexistent clone and
   template produced a fabricated `0/0` inventory and the function returned `Ok(())` —
   a "validated" clone of nothing. Fixed by sourcing the inventory from `pg_namespace`, so a
   missing schema produces no row and the existing guards fire.

Because `perf/test-isolation-template` was declared "verified, mergeable" by its own P1-B
report *before* this review, defect 1–3 are live there. Backporting `67fd21cc` is recorded as
an open follow-up in the SDD ledger and is **not** done by this task.

---

## 4. The seed-row gap the shared extraction introduced

`CREATE TABLE ... (LIKE ... INCLUDING ALL)` copies **structure, not rows**. The v11 baseline
seeds three tables with `INSERT`s, measured in the live template for this record:

| table | seeded rows |
|---|---|
| `sync_stream_id` | **4** (`events`, `presence`, `receipts`, `account_data`) |
| `server_retention_policy` | **1** |
| `server_media_quota` | **1** |

The old storage fixture applied the baseline statement-by-statement, so it *did* have those
rows; replacing it with a `LIKE`-based clone silently lost them. The gap surfaced as a
retention failure and was initially band-aided **in the test** (`retention_service.rs`
re-inserting `server_retention_policy`), which left `sync_stream_id` and `server_media_quota`
unseeded in every clone. Fixed at the source: a general phase-1b template→clone row copy in
the shared module (`8969bd0a`), with the test-level band-aid removed, and both consumer
suites re-verified afterwards. The root-crate legacy fixture (`src/test_utils.rs`) still uses
its own curated seed allowlist; that is deliberately out of scope.

---

## 5. Full-gate runs

Exact command, run twice:

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust-wt-unify
export SQLX_OFFLINE=true CARGO_TARGET_DIR=/tmp/wt_unify_target
export TEST_DATABASE_URL="postgresql://synapse:d3948c491e7dfaccc848b3568bf1bee7@192.168.107.2:5432/synapse_test"
export DATABASE_URL="$TEST_DATABASE_URL"
export REDIS_URL='redis://localhost:6379'
unset SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE
cargo nextest run --workspace --lib --all-features --locked --test-threads 4 \
  -E 'not test(/^media::tests::/)' --no-fail-fast
```

| Run | Summary (verbatim) | Wall window (UTC) |
|---|---|---|
| 1 | `Summary [ 611.613s] 6162 tests run: 6161 passed, 1 failed, 13 skipped` | 08:18:31 → 08:35:59 (includes compile) |
| 2 | `Summary [ 692.939s] 6162 tests run: 6161 passed, 1 failed, 13 skipped` | 08:36:05 → 08:47:40 |

**The two runs' `passed`/`failed` numbers match: 6161 passed / 1 failed in both**, and the
single failure is the same test in both runs
(`synapse-rust server::tests::render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary`).
That is the determinism the branch set out to achieve, against the pre-change drift of
`6128/0`, `6116/4`, `6117/3`.

Non-accepted failures: **none.** The only failure in either run is the accepted appservice
test listed in §6; `synapse-common time::tests::test_calculate_age_near_zero` passed in both
runs.

---

## 6. Accepted, still-unfixed failures

These are known, documented and explicitly out of scope for this plan. They are **not** fixed
or weakened here.

| Failure | Status in this record | Why it is out of scope |
|---|---|---|
| `synapse-common time::tests::test_calculate_age_near_zero` | **passed in both runs**; load-sensitive (fails when `calculate_age(now)` is delayed ~6 ms under 4-way concurrency, passes alone) | Clock tolerance in `synapse-common`; unrelated to test isolation; accepted flake in P1-B §5c② |
| `synapse-rust server::tests::render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary` | **failed in both runs** — the only failure: `src/server/mod.rs:1215:53: shared test pool should be available: "refusing to DROP SCHEMA public on a database that looks deployed (public.schema_migrations exists)..."` | Its shared-pool setup is refused by the `public`-wipe guard; that is exactly the correct, fail-safe behaviour, and the guard is intentional (P1-B §5c③). Fixing it means changing the guard or that test's pool, both outside this plan |
| `synapse-services media::tests::*` | **absent from both runs by construction** — the gate selector excludes `^media::tests::` | These tests build their own partial schema in `prepare_media_test_pool` (`synapse-services/src/media/mod.rs:700`) and still fall back to `public`; their 3 failures are deterministic when run alone and structurally independent of this task (SDD ledger, Task 5 correction) |

---

## 7. Where the earlier per-task evidence lives

The full SDD ledger and the per-task reports are gitignored scratch — **not committed** —
under:

- `.superpowers/sdd/2026-09-13-unify-test-isolation/progress.md` — the ledger: per-task
  status, review rounds, deferred minors, and the cross-task constraints (fingerprint concat
  order; the `perf/test-isolation-template` backport follow-up).
- `.superpowers/sdd/2026-09-13-unify-test-isolation/task-N-report.md` — per-task reports,
  including `task-3-report.md` (the three defects, with before/after SQL evidence),
  `task-5-report.md` / `task-5-fix1-report.md` (the seed-row root cause and fix), and
  `task-6-report.md` (the extraction guards).
- `.superpowers/sdd/2026-09-13-unify-test-isolation/review-*.diff` — per-task review diffs.

The earlier P1-B record for the sibling branch lives at
`docs/audit/P1B_test_isolation_template_2026-09-13.md` on `perf/test-isolation-template`
(`cd13acaf`), not on this branch.

---

## 8. Reproduction

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust-wt-unify
export SQLX_OFFLINE=true CARGO_TARGET_DIR=/tmp/wt_unify_target
export TEST_DATABASE_URL="postgresql://synapse:d3948c491e7dfaccc848b3568bf1bee7@192.168.107.2:5432/synapse_test"
export DATABASE_URL="$TEST_DATABASE_URL"
export REDIS_URL='redis://localhost:6379'
unset SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE

# Gate run 1 / run 2 (run both; summaries must match)
cargo nextest run --workspace --lib --all-features --locked --test-threads 4 \
  -E 'not test(/^media::tests::/)' --no-fail-fast 2>&1 | tail -6

# Final checks
cargo fmt --all
./scripts/check_fmt_ratchet.sh 2>&1 | tail -2
cargo clippy --workspace --all-targets --all-features --locked 2>&1 | grep -cE "^error"
git status --porcelain
git log --oneline de3df1fb..HEAD
```

Gate results: `6161 passed / 1 failed / 13 skipped` in both runs (the one failure is the
accepted appservice test). Final checks: `fmt debt: current=0 baseline=0` (OK),
`clippy errors: 0`, working tree clean, history as listed in §9.

---

## 9. Implementation commits on this branch (through `e4823a8b`)

This closing record is the commit that adds this file
(`docs(audit): record unified test isolation results and remaining known flakes`).

```
e4823a8b test(guards): close the fixture-replay coverage hole and pin the baseline fingerprint
30fd3949 test(guards): isolation fixtures must use the shared template module
8969bd0a fix(test-infra): clone template row data, not just structure
8ba3ba64 fix(test-infra): services isolated pool clones the full baseline, not an empty schema
d43b6ef2 refactor(test-infra): storage isolation fixture delegates to synapse-common
67fd21cc fix(test-infra): correct clone view ordering, trigger retargeting, and validation
f986ee47 feat(test-infra): add clone_schema_from_template with inventory validation
e51b6c46 fix(test-infra): make template reuse assertion load-bearing and stop silent DB-test skips
f0baad8b chore(test-infra): land the template-clone reference implementation before extraction
f61e2ae7 feat(test-infra): add ensure_template_schema with advisory lock and readiness marker
7c5692e0 test(test-infra): strengthen synapse-common test_isolation coverage
f521d021 feat(test-infra): extract baseline fingerprint + SQL splitter into synapse-common
```
