# synapse-rust Spec-Alignment & Hardening Plan (2026-05-01)

This document captures the gaps found while driving the Element Web client
through our homeserver in a live stack, and the remediation we are applying.
Upstream reference: <https://github.com/element-hq/synapse> (Python reference
implementation; Matrix C-S spec v1.13 is the authoritative source).

## 0. Scope

This is the **second-pass** audit, focused on issues visible end-to-end — not
unit test coverage. We drove Element until we hit a broken flow, then dug down
to the first upstream-visible cause. Most of what follows was uncovered while
fixing the "Unsupported algorithm undefined" key-backup regression.

Symptom-level triggers that revealed systemic issues:

- `GET /_matrix/client/v3/room_keys/version` returning `{versions:[...]}` instead
  of the spec-defined flat object.
- `PUT /_matrix/client/v3/room_keys/keys/{room}/{session}?version=...` returning
  `405 Method Not Allowed` with `Allow: GET,HEAD`.
- `count` field in key-backup responses silently reporting `2` when `3` sessions
  had been stored.
- `is_verified` round-trip returning `false` even when clients sent `true`.
- Container running a stale image despite successful local rebuilds.

Each of those is the tip of a pattern that repeats in other subsystems. This
plan enumerates them and assigns concrete fixes.

## 1. Findings

### 1.1 Router wiring is not enforced

`src/web/routes/key_backup.rs::create_key_backup_router` exported a fully
populated router, but `src/web/routes/assembly.rs` never called `.merge(...)`
on it. The routes it defined were dead for months; the *actual* key-backup
endpoints were served by stale, ad-hoc handlers in
`src/web/routes/e2ee_routes.rs` that happened to be wired in the assembly.

**Impact**: whichever file gets edited first wins, silently. Fixes to
`key_backup.rs` have no effect until someone edits `assembly.rs`; readers
assume the routes are live because the file "looks" wired.

**Upstream parity**: in element-hq/synapse, every REST servlet module registers
itself through a single `register_servlets(hs, http_server)` entry point that
`synapse.rest.__init__.register_servlets` calls explicitly. Missing a register
call is caught in tests because the integration harness exercises every
documented endpoint.

### 1.2 Multiple routers register overlapping paths

`e2ee_routes.rs` and `key_backup.rs` both registered
`/_matrix/client/v3/room_keys/version`, `/room_keys/version/{version}`,
`/room_keys/keys`, `/room_keys/keys/{room_id}`, and
`/room_keys/keys/{room_id}/{session_id}` under the same nested prefixes. Axum
does not reject this at merge time for method-distinct handlers; the merge
result is whichever route wins by insertion order. In our case
`e2ee_routes` won, and because it only registered `GET` on `/keys/{room_id}`
and `PUT` on `/keys/{room_id}/{session_id}`, PUTs to the former and DELETEs
everywhere returned `405`.

**Impact**: 405/404 mysteries that no amount of handler-level debugging will
explain.

### 1.3 Matrix spec divergence on `/room_keys/*`

Matrix C-S §11.13 places the backup version in a query string on every
`/room_keys/keys[*]` endpoint. Our router used path segments
(`/room_keys/keys/{version}/...`). Element always sends the spec form, so PUT
uploads went to a route that expected PUT bodies shaped
`{sessions: [Value, ...]}` (array) instead of the spec shape
`{sessions: {sessionId: KeyBackupData}}` (object map). `serde::Deserialize`
silently produced an empty vector and the server reported "success" while
persisting nothing.

Further sub-gaps:

- `PUT /room_keys/version/{v}` returned `{version: "..."}`; spec response is
  `{}`.
- `GET /room_keys/version` without a stored backup returned `{versions:[]}`
  200; spec is `404 M_NOT_FOUND`.
- Write responses omitted `{etag, count}`.
- `algorithm` default fallback was `m.megolm.v1.aes-sha2` — the event
  encryption algorithm, **not** the backup algorithm
  `m.megolm_backup.v1.curve25519-aes-sha2`.
- Session delete helpers were scoped by `(user_id, room_id, session_id)`
  without limiting to the requested `version`, so deleting one version
  erased all backup keys for that room.

### 1.4 Persistence shape loses KeyBackupData fields

`backup_keys (backup_id, room_id, session_id, session_data jsonb, created_ts)`
has no columns for `first_message_index`, `forwarded_count`, or `is_verified`.
Read paths synthesized them as literal `0, 0, FALSE`, and write paths wrapped
the spec body as `{"session_data": stringified_session_data}` which
lost the three metadata fields entirely.

### 1.5 Duplicate source-of-truth for auth/algorithm constants

Two concurrent copies of backup-upload logic live in the tree:

- `e2ee_routes::put_room_keys` / `put_room_key` (now orphaned)
- `key_backup::put_room_keys_all` / `put_room_keys_for_room` / `put_room_key`
  (live)

They disagreed on algorithm defaults, etag formats, and field handling.
Upstream Python synapse keeps one servlet per resource.

### 1.6 Deploy path can silently run stale image

`docker/docker-compose.yml` references `synapse-rust:latest` by default, but
`docker/.env` overrides `SYNAPSE_IMAGE=vmuser232922/mysynapse` and
`SYNAPSE_IMAGE_TAG=0.1.0-amd64`. `docker buildx ... -t synapse-rust:latest
--load` produces an image nobody consumes; `docker compose up --force-recreate
synapse-rust` happily recreates from the `vmuser...` tag that was not
rebuilt. We shipped "fixed" binaries that were never actually running for
more than a day.

### 1.7 Observability gaps

- No integration test exercises the backup PUT/GET/DELETE lifecycle with a
  real HTTP harness. The unit tests in `tests/unit/key_backup_api_tests.rs`
  only assert JSON shapes in isolation, so every bug in §1.3 passed CI.
- Startup logs do not enumerate merged routers, so router drift is invisible.
- `docker logs synapse-rust` did not surface the 405 path (it was 200-level
  from the client's perspective when ending up on a mismatched handler).

## 2. Remediation plan

Tracked in this document; commit references are in the accompanying PR(s).
Priority labels: **P0** (must ship to unblock Element E2EE), **P1** (must
ship within the sprint), **P2** (hygiene).

| # | Area | Item | Priority | Status |
|---|------|------|----------|--------|
| R1 | Routing | Wire `create_key_backup_router` in `assembly.rs` | P0 | done |
| R2 | Routing | Remove duplicated key-backup routes from `e2ee_routes.rs` | P0 | done |
| R3 | Routing | Delete orphan handlers in `e2ee_routes.rs` (`create_room_keys_version`, `get_room_keys_version`, `get_room_keys_version_by_id`, `delete_room_keys_version`, `put_room_keys`, `put_room_key`, `get_room_key`, `get_room_keys`) | P1 | done |
| R4 | Routing | Startup-time duplicate-path guard: walk the assembled `Router` and panic if the same `(method, path)` tuple is registered twice | P1 | done — substituted by an explicit route ledger (`src/web/routes/route_ledger.rs`). `assembly::declared_route_manifest_for(&AppState)` now aggregates three layers: (1) assembly inline routes, (2) explicit always-on manifests (`assembly_compat_manifest`, client/admin submodules, and the always-on `worker` admin subset via `worker::worker_route_manifest()` after the admin/body split landed), and (3) `route_module::route_modules()` for state-aware / feature-gated routers (`room`, `federation`, `oidc`, `worker_body` behind `config.worker.enabled`, `saml`, `cas`, `widget`, `burn_after_read`, `friend`, `voice`, `external_service`, `ai_connection`, `openclaw`). `assembly_compat_manifest()` also covers the compile-time `voip-tracking` branch via `assembly::voip_tracking`, so there is no known feature-gated merge/route surface left outside the ledger. Runtime coverage is guarded by `declared_route_manifest_validates_with_no_duplicates`, `declared_route_manifest_entries_are_actually_wired`, `declared_route_manifest_key_route_snapshot_matches_default_state`, `declared_route_manifest_key_route_snapshot_matches_worker_enabled_state`, `worker_body_routes_follow_runtime_flag_in_ledger`, and `openclaw_routes_follow_runtime_flag_in_ledger`; feature-gated declaration tests guard `friends`, `voice`, `widget`, `external_service`, `burn_after_read`, `cas`, and `voip_tracking`, while `route_module` unit tests pin the manifest cores for the modular gated routers. There is no remaining route surface outside the ledger: the `worker_router` body branch is now reported by `worker::worker_body_route_manifest()` and merged via `route_module::WorkerBodyModule`, so the duplicate guard and live probe cover it whenever `config.worker.enabled` is set. |
| S1 | Spec | `GET /room_keys/version` returns flat latest-backup object or 404 | P0 | done |
| S2 | Spec | `GET /room_keys/version/{v}` adds `{count, etag}` | P0 | done |
| S3 | Spec | Routes `/room_keys/keys[/{room}[/{session}]]` accept `?version=` | P0 | done |
| S4 | Spec | Legacy path-version routes retained for MSC/backwards compat | P0 | done |
| S5 | Spec | Body shapes use keyed maps, not arrays | P0 | done |
| S6 | Spec | All writes return `{etag, count}` | P0 | done |
| S7 | Spec | Version-scoped deletes (`delete_session_for_version`, `delete_room_for_version`, `delete_all_for_version`) | P0 | done |
| S8 | Spec | `algorithm` default falls back to `m.megolm_backup.v1.curve25519-aes-sha2` (not the event-encryption algorithm) | P1 | done |
| P1 | Persistence | Promote `first_message_index / forwarded_count / is_verified` to real columns so Postgres can index and query them; remove the JSON-wrapping workaround | P1 | done (migration `20260501000001_backup_keys_metadata.sql`, storage + service readers updated) |
| T1 | Testing | Promote `/tmp/kb_e2e_test.sh` into the repo (`tests/e2e/scripts/key_backup.sh`) and invoke it from `scripts/run_ci_tests.sh` | P1 | done — script lives under `tests/e2e/scripts/` and is invoked from `scripts/ci_backend_validation.sh::run_docker_smoke` |
| T2 | Testing | Rust integration test that starts an in-process axum app and verifies the route table contains the 14 key-backup endpoints with the right methods | P2 | done — `tests/integration/api_key_backup_route_table_tests.rs` probes each path with `PATCH` and asserts the 405 `Allow` header lists the expected methods (substitutes for R4) |
| O1 | Ops | Add a `make docker-build` target that tags both `synapse-rust:latest` and `${SYNAPSE_IMAGE}:${SYNAPSE_IMAGE_TAG}` from the `.env` file | P1 | done |
| O2 | Ops | `server.rs` startup log should print the count of unique `(method, path)` tuples and highlight any duplicates | P2 | done — `create_router` emits `route manifest validated: N declared (method, path) tuples, 0 duplicates` via `tracing::info!` (target `synapse_rust::routes`). Duplicates abort startup with the offending entries listed. Coverage is explicit and state-aware per R4 rather than a blind walk of the assembled axum router. |

### 2.1 Next-round hardening backlog (post-R4)

The original remediation items above are complete. The next round is no longer
about recovering missing routes; it is about making the route-ledger guardrails
harder to regress and cheaper to trust in CI/local runs.

| # | Area | Item | Priority | Status |
|---|------|------|----------|--------|
| N1 | Testing | Eliminate integration-test DB setup skips caused by template cloning pressure (`out of shared memory`) so route-ledger and adjacent suites fail deterministically instead of silently degrading to `Skipping` | P1 | done — shared template cloning is now throttled by `TEST_DB_SHARED_CLONE_CONCURRENCY` (default `2`) in `src/test_utils.rs`, and per-test schema names include a timestamp suffix to avoid process-id reuse collisions. `tests/integration/mod.rs` still keeps the isolated fallback as a safety net, but `cargo test --test integration api_route_ledger_tests -- --nocapture` now completes without shared-clone fallback messages. |
| N2 | Testing | Add a worker-enabled live-router probe that exercises the `worker_body` surface against an assembled app with `config.worker.enabled = true`, complementing the current ledger-only assertion | P1 | done — `tests/integration/api_route_ledger_tests.rs::worker_body_routes_are_live_when_worker_mode_enabled` now asserts the same worker-body endpoints are `404` when `worker.enabled = false` and become `401` (replication-auth protected) when `worker.enabled = true`, proving the live router assembly path is active. |
| N3 | Testing | Promote the current inline key-route snapshots into an auto-exported full route-ledger snapshot artifact (sorted, committed, diff-friendly) so unexpected route-surface changes show up as a single reviewable diff | P1 | done — `tests/integration/api_route_ledger_tests.rs` now renders full sorted snapshots for the default state and the `worker.enabled = true` state, and compares them against committed artifacts under `tests/integration/snapshots/`. Setting `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` regenerates the baseline files. |
| N4 | Ops | Extend startup route-manifest logging with per-`registered_by` namespace counts (or top-level module summaries) so router drift can be localized without reproducing the full integration probe locally | P2 | done — `RouteLedger::registered_by_counts()` now produces a stable per-namespace summary, and `create_router` logs both the total tuple count and a `registered_by_summary` field alongside `registered_by_namespaces`. |
| N5 | Tooling | Add a lightweight contributor guardrail to the routing docs/comments: new feature-gated routes must land with either a `RouteModule` or an explicit `*_route_manifest()` update in the same PR | P2 | done — the contributor rule is now documented in `src/web/routes/route_ledger.rs` and `src/web/routes/route_module.rs`: feature-gated route changes must update the ledger, startup summary, and route-ledger snapshots in the same PR. |

#### Recommended execution order

1. **N1 first** — test infrastructure determinism. As long as route-ledger runs
   can degrade to `Skipping` because Postgres template cloning is under memory
   pressure, every other hardening item has reduced signal.
2. **N2 second** — close the one remaining runtime-validation gap. The ledger
   already knows about `worker_body`, but we still want one live assembled-app
   probe with `config.worker.enabled = true` so the route table and auth stack
   are exercised together.
3. **N3 third** — promote route-surface reviewability. Once the suite is stable
   and the last runtime-gated branch has a live probe, export the full ledger
   into a committed snapshot artifact so route churn is visible in code review.
4. **N4 fourth** — improve startup observability. Namespace-level counts are
   lower risk and easier to validate after the route surface and snapshots are
   already stable.
5. **N5 last** — codify the contributor rule after the technical guardrails are
   in place, so the docs point at real enforcement mechanisms rather than
   aspirational process.

#### Exit criteria

- **N1**
  - `tests/integration/api_route_ledger_tests.rs` and adjacent route suites no
    longer print best-effort `Skipping` in the normal local/CI path.
  - A schema/bootstrap failure becomes a deterministic failure, or the harness
    is reworked so the setup path stays within Postgres shared-memory limits.
- **N2**
  - At least one integration test assembles the app with
    `config.worker.enabled = true` and confirms selected `worker_body` routes
    are live in the router, not just present in the ledger.
- **N3**
  - A single sorted snapshot artifact can be regenerated from
    `declared_route_manifest_for(&AppState)` for the default state and selected
    alternate states.
  - Route-surface changes appear as a plain diff instead of being spread across
    hand-maintained inline assertions.
- **N4**
  - Startup logs show both the total unique tuple count and a compact
    per-namespace/per-module summary derived from `registered_by`.
- **N5**
  - `route_ledger.rs`, `route_module.rs`, or adjacent routing docs explicitly
    state the rule that every new feature-gated route must update the ledger in
    the same change.

## 3. Specification cross-reference table (§11.13 key backup)

| Endpoint | Matrix C-S | Our spec route | Our legacy route |
|----------|------------|----------------|------------------|
| `POST /room_keys/version` | create | `/room_keys/version` | — |
| `GET  /room_keys/version` | latest | `/room_keys/version` | — |
| `GET  /room_keys/version/{v}` | specific | `/room_keys/version/{version}` | — |
| `PUT  /room_keys/version/{v}` | update auth_data | `/room_keys/version/{version}` | — |
| `DELETE /room_keys/version/{v}` | delete | `/room_keys/version/{version}` | — |
| `GET  /room_keys/keys?version=` | read all | `/room_keys/keys` + `VersionQuery` | `/room_keys/{version}/keys` |
| `PUT  /room_keys/keys?version=` | write all | same | same |
| `DELETE /room_keys/keys?version=` | delete all | same | same |
| `GET  /room_keys/keys/{room}?version=` | read room | `/room_keys/keys/{room_id}` | `/room_keys/{version}/keys/{room_id}` |
| `PUT  /room_keys/keys/{room}?version=` | write room | same | same |
| `DELETE /room_keys/keys/{room}?version=` | delete room | same | same |
| `GET  /room_keys/keys/{room}/{session}?version=` | read session | `/room_keys/keys/{room_id}/{session_id}` | `/room_keys/{version}/keys/{room_id}/{session_id}` |
| `PUT  /room_keys/keys/{room}/{session}?version=` | write session | same | same |
| `DELETE /room_keys/keys/{room}/{session}?version=` | delete session | same | same |

## 4. Migration applied for §1.4

Shipped as `migrations/20260501000001_backup_keys_metadata.sql`:

```sql
-- migrations/20260501000001_backup_keys_metadata.sql
ALTER TABLE backup_keys
    ADD COLUMN IF NOT EXISTS first_message_index BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS forwarded_count     BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_verified         BOOLEAN NOT NULL DEFAULT FALSE;

-- Backfill from any existing JSON payload that already carries these fields.
UPDATE backup_keys
SET    first_message_index = COALESCE((session_data ->> 'first_message_index')::BIGINT,  first_message_index),
       forwarded_count     = COALESCE((session_data ->> 'forwarded_count')::BIGINT,      forwarded_count),
       is_verified         = COALESCE((session_data ->> 'is_verified')::BOOLEAN,         is_verified)
WHERE  jsonb_typeof(session_data) = 'object';
```

The storage and service layers no longer wrap the three metadata fields inside
`session_data`; `BackupKeyInsertParams` writes them straight to the dedicated
columns and every read path projects `bk.first_message_index`,
`bk.forwarded_count`, `bk.is_verified` (verified by grepping
`src/e2ee/backup/{storage,service}.rs` for `session_data ->>` — no remaining
JSON-extraction call sites). `session_data` now stores only the opaque
ciphertext, matching upstream synapse's schema.

## 5. Follow-ups explicitly NOT in scope

- Replacing the JWT token backend with OIDC is tracked separately.
- Sliding sync MSC4186 parity work is tracked in `docs/synapse-rust/api/`.
- Federation retries / backoff rework is tracked in `PERFORMANCE_BASELINE.md`.

## 6. Verification

After the P0 / P1 items land:

1. `cargo test --test integration key_backup` must pass.
2. `bash tests/e2e/scripts/key_backup.sh` must pass against a fresh stack
   brought up by `docker compose -f docker-compose.yml -f
   docker-compose.web.yml up -d`.
3. Startup log must print `route manifest validated: N declared (method, path) tuples, 0 duplicates` (target `synapse_rust::routes`). Duplicates abort startup before the listener binds.
4. Element Web must complete the "Set up secure messaging" flow end-to-end
   against a fresh user without any 4xx in the console for
   `/_matrix/client/v3/room_keys/*`.

## 7. Operational notes

### 7.1 Compat-layer disposition

The four Matrix compat routers (`auth_compat`, `account_compat`,
`directory_compat`, `voip_compat`) plus the adjacent `capabilities` and
`media_config` nests are **kept, not deprecated**. They are load-bearing:

- Spec v1.13 still requires both `r0` and `v3` paths for these endpoints;
  clients in the wild (including Element Web, Element Android, and older
  third-party clients) continue to mix versions.
- `media_config` must remain reachable under `v1`, `r0`, and `v3`
  simultaneously because the v1.11 media spec split was additive.
- The compat surfaces are now fully ledger-tracked via
  `assembly_compat_manifest()` with explicit `registered_by` namespaces
  (`assembly::auth_compat`, `assembly::account_compat`,
  `assembly::directory_compat`, `assembly::voip_compat`,
  `assembly::voip_tracking`, `assembly::capabilities`, `assembly::media_config`,
  `assembly::account_r0_only`, `assembly::directory_r0_only`,
  `assembly::auth_router`). The duplicate guard covers them end-to-end;
  no separate "compat deprecation" work is planned.

No feature-gated routes remain scattered in `assembly.rs` without ledger
coverage: the runtime-gated `worker` body surface is declared through
`route_module::WorkerBodyModule`, and the compile-time `voip-tracking`
compat handlers are declared directly from `assembly_compat_manifest()`.

Retiring them would require a Matrix spec sunset of the `r0` prefix —
which is not on the standards roadmap — and would break every client that
still probes the legacy paths. They stay.

**Verification snapshot (2026-05-02)** — re-evaluation of "deprecate
with old versions to avoid pointless migration" concluded "do not
deprecate". Concrete evidence:

- `rg -lc '/_matrix/client/r0/' tests/` reports 200+ direct callers
  spread across `api_room_tests` (94), `api_e2ee_tests` (19),
  `api_placeholder_contract_p1p2_tests` (18), `api_profile_tests`,
  `api_placeholder_contract_p0_tests`, `api_device_presence_tests`,
  `api_enhanced_features_tests`, `api_route_ledger_tests`,
  `api_room_summary_routes_tests`, `rate_limit_config_tests`, and
  more. The integration suite exercises r0 as a first-class surface;
  removing it would invalidate a large coverage population.
- The compat manifests are ledger-tracked, so they cost nothing in
  ongoing maintenance — the duplicate guard fires automatically if a
  v3 handler shadows an r0 handler or vice versa.
- The supposed savings of "removing dead compat code" don't exist:
  every compat handler is a thin re-route to the same v3 service
  layer. Deleting the routers wouldn't shrink the service-layer
  contract, only the URL surface.

Re-evaluate only when **all three** of the following are true:
(a) Matrix spec marks `r0` as `deprecated` in its compatibility
notes, (b) Element Web/Android/iOS releases drop r0 calls in their
HTTP layer, and (c) our access logs (production) report zero r0
hits over a rolling 30-day window. Until then, keep the routers
and the ledger entries. Treat any PR that removes r0 routes as a
breaking change requiring spec citation in the description.

### 7.2 Ledger probe test scaling

`tests/integration/api_route_ledger_tests.rs::declared_route_manifest_entries_are_actually_wired`
probes every declared entry via `PATCH`. **Parallelized 2026-05-02** —
`futures::stream::iter(entries).buffer_unordered(PROBE_CONCURRENCY)`,
`PROBE_CONCURRENCY = 16` (matched against the integration test PG
pool). Live datapoints (2026-05-02):

| Tree | Manifest size | Probe-only | Full ledger+key_backup suite |
|------|---------------|------------|------------------------------|
| `hu_ts` | 1190 routes | 65 s | 158 s |
| `hu`    | 1001 routes | 76 s | (probe-only measured) |

Follow-up datapoint (2026-05-02, same `hu_ts` tree): after adding
`OnceCell` fixture caching for the default / worker-enabled /
openclaw-enabled app-state variants and caching the derived
`RouteLedger`, `cargo test --test integration api_route_ledger_tests --
--nocapture` completes in 99.17 s locally. The exact live-probe test
(`declared_route_manifest_entries_are_actually_wired`) still lands at
~69 s, so this round's win comes from setup/manifest reuse rather than
from a change to `PROBE_CONCURRENCY` or the PATCH fan-out itself.

A regression guard (`declared_route_manifest_size_stays_under_probe_warning_threshold`)
ships in both trees with a per-tree `WARNING_ROUTE_COUNT` ceiling
(1300 in `hu_ts`, 1100 in `hu` — current size + ~10% headroom).
Crossing it forces a conscious revisit of this section before the
constant is bumped.

Budget triggers:

- **Warning threshold** — > 120 s probe-only wall time, or manifest size
  exceeds the per-tree `WARNING_ROUTE_COUNT`. Re-inspect the trade-off
  below before raising the constant.
- **Action threshold** — > 180 s probe-only wall time, or CI flake rate
  > 2%. Implement one of:
  1. **Increase concurrency** — bump `PROBE_CONCURRENCY` past 16, but
     only after raising `database.max_connections` in the test config;
     otherwise the probe deadlocks on pool acquisition. Preferred first
     step.
  2. **Deterministic sample** — probe 100% of manifest entries whose
     `registered_by` changed vs `git merge-base origin/main`, and a
     fixed 10% stratified sample of the rest (stratified by
     `registered_by` namespace so no module escapes scrutiny for long).
     Reduces latency but loses the "every route every run" property —
     only adopt if pool-bound concurrency is insufficient.
  3. **Manifest-only fallback** —
     `declared_route_manifest_validates_with_no_duplicates` and
     `declared_route_manifest_size_stays_under_probe_warning_threshold`
     are both sub-second and keep running unconditionally. If the live
     probe must be skipped entirely, keep the duplicate + size guards
     as gating tests and move the live probe to a nightly job.

Do not weaken the duplicate-validation test under any scenario — it is
the direct defense against the original key-backup router-wiring bug.

### 7.3 Persistence hygiene — §1.4 follow-ups

The §1.4 fix (promote `first_message_index / forwarded_count /
is_verified` from inside `session_data` jsonb to dedicated columns) is
**structurally complete** as of 2026-05-02:

- `session_data ->> 'first_message_index'` / `'forwarded_count'` /
  `'is_verified'` extraction call sites: zero remaining in
  `src/e2ee/backup/{storage,service}.rs` (verified by
  `rg "session_data ->>"`).
- No SELECT in active code synthesises any of the three values as
  literal `0`/`FALSE`. (`rg "0::BIGINT AS|FALSE AS is_verified"`
  returns no hits outside test fixtures.)

**Audit criteria for future §1.4-style migrations.** A jsonb column is
a candidate for promotion when *all* of the following hold:

1. The DTO that lands in the column has a stable, spec-defined set of
   named fields (not a free-form blob).
2. At least one read path needs to filter, sort, or aggregate by one
   of those named fields — i.e. the operation needs an index.
3. The current query plan reaches that field via `->>` and a runtime
   cast (e.g. `(col ->> 'k')::BIGINT`).
4. Loss of the value would be silent — dropped on round-trip without
   raising an error — making bugs visible only as wrong query results.

Migration shape mirrors `20260501000001_backup_keys_metadata.sql`:
`ADD COLUMN IF NOT EXISTS … DEFAULT …`, backfill from the existing
JSON via `UPDATE … SET col = COALESCE((blob ->> 'k')::T, col)`,
then strip the field from the storage layer's write path.

**Surveyed columns explicitly NOT candidates** (Matrix spec opaque or
genuinely free-form):

| Table.column | Why kept as jsonb |
|--------------|-------------------|
| `events.content`, `events.unsigned`, `events.prev_events`, `events.auth_events`, `events.signatures`, `events.hashes` | Matrix C-S §3.4 mandates JSON object shape; canonical form is the JSON. |
| `state_events.content`, `account_data.content`, `device_messages.content` | Spec-defined opaque event payloads. |
| `cas_*.allowed_attributes` / `allowed_proxy_callbacks`, `saml_*.attributes` / `attribute_mapping` | Free-form attribute bags scoped per-deployment; no fixed schema to extract. |
| `application_services.config`, `module_definitions.config`, `worker_*.config`, `webhooks.headers` | Per-instance configuration; key set is open. |
| `room_summary.summary`, `presence.metadata`, `task_*.task_data` / `result` | Internal cache/result blobs without query predicates. |
| `friend_room_events.content -> 'friends'` | A persistence of a Matrix event payload — promoting `friend_id` would denormalise the canonical event content. The friends list **is** mirrored into a `friends` table for the relational query path; the jsonb copy is the canonical event storage. |
| `audio_messages.waveform` (voice extension) | Compact float array; never joined or filtered. |

If a future PR wants to add a new jsonb column whose contents already
satisfy the four audit criteria above, prefer adding the dedicated
column from day one rather than shipping the jsonb form first and
migrating later. Each round-trip through "land in jsonb, then promote"
is a §1.4-shaped bug waiting to happen.
