# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Common commands

### Local Rust development
- Build: `cargo build --locked`
- Run server: `SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --release`
- Run worker binary: `cargo run --bin synapse_worker`
- Format check: `cargo fmt --all -- --check` — but the **real CI gate** is `./scripts/check_fmt_ratchet.sh` (ratchet with baseline 0, counts diff blocks via `grep -c '^Diff in'`; both `current > baseline` and `current < baseline` fail, so after `cargo fmt --all` you are at `current=0=baseline` and pass).
- Clippy: `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings`
- ⚠️ CI clippy does NOT cover test code across the workspace (only `-p synapse-services --tests`); `cargo clippy --workspace --all-targets --all-features` may show extra warnings in test code.
- Doc tests: `cargo test --doc --locked` — ⚠️ **this is currently an empty gate** (root crate has 0 doc tests; workspace-wide there are only 4 and all are `#[ignore]`d). A real rustdoc-only compile error (E0106) once shipped green through this gate. Prefer `cargo test --doc --locked --workspace` when touching doc examples, and note rustdoc catches lifetime elision errors that `cargo check`/`clippy` miss entirely.
- Full test suite: `cargo test --all-features --locked -- --test-threads=4`
- CI-equivalent Rust test entrypoint: `TEST_THREADS=4 TEST_RETRIES=2 bash scripts/run_ci_tests.sh`
- If `cargo-nextest` is installed, `scripts/run_ci_tests.sh` uses it automatically; otherwise it falls back to `cargo test` with retries.
- `cargo nt` is a repo alias (`.cargo/config.toml`) for `cargo nextest run --profile test --features test-utils`. It works for `--lib`/`--test unit` but **not for `--test integration`** (nextest 0.9.140 silently ignores `features` in profiles + the integration target has `required-features`); use the explicit `--all-features` command below for integration.

### Running specific tests
- Unit test target: `cargo test --test unit`
- Integration tests: **`cargo nextest run --profile ci --all-features --test integration --test-threads 1`** ⚠️ `--all-features` is REQUIRED (CI port), otherwise `/login` snapshots, route ledger tests, and other feature-gated tests will fake-fail.
- E2E target: `cargo test --test e2e`
- Performance manual target: `cargo test --features performance-tests --test performance_manual -- --nocapture`
- Run one named integration test: `cargo nextest run --profile ci --all-features --test integration <test_name> -- --nocapture`
- Compile one integration test target without running DB setup: `cargo test --features test-utils --all-features --test integration <test_name> --no-run`
- Run one unit test from the unit target: `cargo test --test unit <test_name> -- --exact --nocapture`
- Run one library unit test by substring: `cargo test --lib <test_name> -- --nocapture`
- Single test with TDD profile (mock support): `cargo nextest run -p <crate> <test_name> -P tdd --features test-utils`

### Benchmarks and coverage
- API benchmark compile/run path: `cargo bench --bench performance_api_benchmarks --no-run`
- Federation benchmark compile/run path: `cargo bench --bench performance_federation_benchmarks --no-run`
- Coverage: CI uses `cargo llvm-cov --workspace` (tarpaulin was replaced). Local end-to-end run: `bash scripts/run_local_coverage.sh` (~15 min, ~68% line coverage as of 2026-08).
- Note: Coverage scripts may timeout when DB has accumulated many test schemas (1363 x 255 tables observed). Run `scripts/cleanup_test_schemas.sh` before coverage if needed.

### Database and migrations
- Migration source of truth: `docker/db_migrate.sh` (applies `migrations/`).
- **`migrations/` is the SINGLE source of truth for migration SQL.** The former duplicate directory `docker/deploy/migrations/` was deleted (commit `2b16dc3c`) after it drifted 82 stale files + 13 missing files from the authoritative set. Do NOT recreate any copy — the deploy path mounts `./migrations` directly.
- Naming convention: rollback files use `.undo.sql` suffix (32 existing). A `_undo.sql` (underscore) name is a violation — normalize it.
- Apply migrations locally: `bash docker/db_migrate.sh migrate`
- Validate migrations/schema locally: `bash docker/db_migrate.sh validate`
- Migration verification helpers live under `scripts/` and `migrations/`; prefer existing scripts over ad hoc SQL.
- **Postgres error codes**: `schema "x" does not exist` is SQLSTATE `3F000` (invalid_schema_name), **not** `42P01` (undefined_table). EXCEPTION blocks in DO $$ must catch both.
- **`CREATE OR REPLACE FUNCTION` cannot change parameter names** — parameter names are part of the function signature; an idempotent migration must `DROP FUNCTION IF EXISTS ...(old_signature)` first.
- **Never split migration SQL with `split(';')`** — `$$...$$` dollar-quoted bodies, string literals, and comments contain internal `;` and will produce truncated functions. Use a character-level splitter.

### Docker workflow
Two stacks, different jobs, deliberately side by side:
`docker/docker-compose.yml` (dev/CI: builds in place, services `synapse-rust`/`db`/`redis`,
entrypoint migrations, no nginx — the `backend-validation` and `e2ee-interop` CI workflows
start the stack **by these service names**) and `docker/deploy/docker-compose.yml`
(production full stack: postgres/redis/migrator/synapse/nginx, `deploy.sh`).
- Start dev stack: `cd docker && docker compose up -d --build`
- Validate containerized migrations: `cd docker && docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate`
- Validate schema in container: `cd docker && docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate`
- CI-like local validation: `bash scripts/ci_backend_validation.sh`
- **Config has a single source of truth: `docker/config/`** — both stacks mount it
  (deploy: `../config`, dev: `./config`) and the Dockerfile bakes the same files.
  There is no `docker/deploy/config/` and there must not be one; see
  `tests/unit/config_mount_tests.rs`.

## 项目状态与反冗余铁律（未发布项目，先读这一节）

**项目状态：未发布、无外部用户、无生产数据。** 因此**不存在向后兼容义务**。
这一条决定了下面所有规则：宁可一次性改干净，不要叠加兼容层。

1. **禁止兼容残留。** 不得为"向后兼容"保留 `#[deprecated]` 项、旧路径别名、
   双实现、feature 开关式的死代码、或"先留着以后可能有人用"的中间态。
   改动直接替换原实现。**判据**：如果某符号/分支/配置的唯一存在理由是
   "兼容旧行为"，就删掉它。
2. **同一职责只允许一份实现。** 模板/schema 隔离、schema 清理、配置解析、
   URL 解析这类基础设施出现第二份实现即视为缺陷。新增前先搜索是否已有
   可复用实现（`synapse-common` 是共享基础设施的首选位置）。
   **反例（已发生）**：测试隔离曾经有三份实现，导致同一类 bug 要修三次。
3. **测试/bench 专用依赖必须放 `[dev-dependencies]`。** 不得进 `[dependencies]`，
   否则会污染生产依赖图。新增依赖时必须说明它对生产依赖图的影响。
   **自查**：`cargo machete`。注意它只扫 `[dependencies]`，所以它报"未使用"
   往往意味着依赖放错了段，而不是真的没用——先核实再决定删还是移。
4. **构建产物与派生缓存不入库。** `artifacts/`、`target/`、SQLx 派生缓存等
   一律 gitignore，且不得用 `git add -f` 绕过。**迁移只有一个真相源：
   `migrations/`**；不得在 `artifacts/` 或任何其他目录再放迁移副本。
5. **`cargo metadata` 必须始终可用。** 任何 vendored 或独立 crate 必须在根
   `Cargo.toml` 的 `[workspace] members` 或 `exclude` 中显式声明。否则
   machete / deny / audit / IDE 等**整套**基于 `cargo metadata` 的工具链会
   直接失效。**反例（已发生）**：`vendor/pastey` 未列入 `exclude`，
   `cargo machete` 整体退出——注意该 vendor 本身是必要的（见 `[patch.crates-io]`
   注释），要修的是 workspace 声明，不是删掉它。
6. **薄壳禁止。** 根 crate 的 `src/{services,storage,common}/mod.rs` 只允许
   re-export，且必须有实际使用者。新增只做转发的模块一律合并进对应 workspace crate。
7. **消除冗余优先于绕过问题。** 遇到并发/共享状态类缺陷，先问"能否从结构上
   消除共享"（如 per-test schema 隔离），而不是加锁/串行化/重试去绕过。
   **反例（已修正）**：retention 测试曾用 `max-threads=1` 串行分组绕过全局
   单例行的跨进程 race；正确修法是让每个测试用独立 schema，race 随之消失。
8. **门禁必须自证能变红。** 任何"检查类"门禁（fmt/clippy/覆盖率/自定义守卫）
   在新增或修改后，必须用**故意制造的违规**证明它真的会失败。报"通过"的门禁
   未必在工作。
   **反例（已修正）**：`scripts/check_fmt_ratchet.sh` 曾用
   `cargo fmt --all -- --check | grep -c '^Diff in'` 计数，但 `cargo fmt`
   检测到差异时**只返回非零退出码、不打印 `Diff in` 块**（那是独立 `rustfmt`
   的输出格式），因此计数恒为 0 —— 对 56 个未格式化文件连续 21 个 CI 运行都报
   "fmt debt: current=0 / OK"。改为 `rustfmt --check` 逐文件计数后，插入一个
   未格式化探针文件即可让它变红。
   **推论**：看到"长期 0 违规 / 长期全绿"的门禁，优先怀疑它没在工作，而不是
   相信代码很干净。

## High-level architecture

### Runtime shape
- `src/main.rs` bootstraps config, telemetry/logging, builds `SynapseServer`, and runs the main homeserver process.
- `src/server/mod.rs` is the main composition root. It creates the Postgres pool, runs schema health checks, wires Redis/in-memory cache, builds `ServiceContainer`, configures rate-limit state, and assembles the Axum router.
- The server exposes both client and federation listeners from the same application state.

### Core layering
The root crate's `src/` is reduced to bootstrap + composition (`main.rs`, `server/`, `bin/`, `tasks/`, plus the `common`/`e2ee`/`cache`/`storage` facades); the HTTP surface and business logic live in workspace crates:

- `synapse-web/` (workspace crate): HTTP boundary. Axum routes, extractors, middleware, validators, and Matrix-compatible endpoint assembly, plus the federation glue that depends on the HTTP context (`synapse-web/src/federation/edu.rs`).
- `synapse-services/`: business logic layer (workspace crate). Feature behavior lives here; composition root is `synapse-services/src/container.rs`.
- `synapse-storage/`: persistence layer over PostgreSQL (sqlx), plus schema/health/performance helpers (workspace crate).
- `synapse-e2ee/`, `synapse-federation/`: E2EE crypto and federation transport/auth logic (workspace crates).
- `synapse-cache/`, `synapse-common/`: Redis-backed cache (in-memory fallback) and shared config/logging/security/rate-limit/task-queue utilities (workspace crates).

The root crate's `src/services/` and `src/storage/` are thin shells (`mod.rs` re-exports only) — new logic belongs in the corresponding workspace crate.

The codebase generally follows `route (synapse-web/src/) -> service (synapse-services/) -> storage (synapse-storage/)`, with `AppState`/`ServiceContainer` carrying shared dependencies.

### Router organization
- `synapse-web/src/routes/assembly.rs` is the top-level router assembly point.
- It merges many feature routers under Matrix-compatible prefixes such as `/_matrix/client/*`, `/_matrix/federation/*`, and admin/auxiliary endpoints.
- Middleware layering is centralized here: CORS, security headers, compression, CSRF, and rate limiting.
- Route implementation is split by domain under `synapse-web/src/routes/` and `synapse-web/src/routes/handlers/`.
- Every route must be declared in `synapse-web/src/routes/route_ledger.rs` and its module's `*_route_manifest()`; the ledger guards against silent `Router::merge` path collisions and is exported via `ledger_export.rs` as the SDK contract source.
- Non-standard/private endpoints must use the `/_matrix/vendor/v1` prefix (ISSUE-13 migration), namespaced apart from Matrix stable and MSC identifiers.

### Dependency wiring
- `synapse-services/src/container.rs` is the main dependency graph for application features.
- It constructs storages and services for auth, rooms, sync, sliding sync, E2EE, federation helpers, media, push, moderation, retention, feature flags, worker integration, and more.
- If you need to understand how a feature is actually enabled end-to-end, start at `ServiceContainer::new(...)`, then trace the relevant router and storage.

### Storage and schema model
- Postgres is the primary source of truth.
- `synapse-storage/src/lib.rs` re-exports domain-specific storages; most features have a corresponding storage module.
- `synapse-storage/src/schema_health_check.rs` is part of startup validation. Missing critical tables/columns fail startup. **Exception**: `SYNAPSE_SKIP_SCHEMA_CHECK=true` bypasses all schema health checks at startup (logged at warn level) — this escape hatch exists for emergency recovery scenarios and should never be used in production.
- Runtime DB initialization is intentionally not the default path. The expected migration flow is externalized through `docker/db_migrate.sh`; server startup only performs schema health checks unless `SYNAPSE_ENABLE_RUNTIME_DB_INIT` is explicitly enabled and `SYNAPSE_SKIP_DB_INIT` is not set.

### Configuration model
- Config types live in `synapse-common/src/config/` (the root crate's `src/common/config/` is a thin re-export).
- Main config is file-based (`SYNAPSE_CONFIG_PATH`, default `homeserver.yaml`) with `SYNAPSE_` environment variable overrides using `__` for nesting.
- Docker uses `docker/config/homeserver.yaml` and mounts `docker/config/rate_limit.yaml` — `docker/config/` is the single config source for both compose stacks and for the image; there is no second copy under `docker/deploy/`.
- Search must exist structurally in config; when Elasticsearch is not used it should still be explicitly disabled.
- Prefer `server.server_name`/`ServerConfig::get_server_name()` for Matrix identity. `server.name` exists for compatibility but can differ from the public Matrix server name in delegated or reverse-proxy deployments.
- Federation config has its own `federation.server_name`; when validating local identity, account for all locally accepted names rather than hard-coding one config field.

### Caching and async/background work
- Redis is optional but first-class. When enabled, the server uses Redis-backed cache and task queue infrastructure; otherwise cache falls back to local memory.
- `ScheduledTasks` and task metrics are initialized in `src/server/mod.rs`.
- Worker-related code lives under `src/worker/`, with an additional binary in `src/bin/synapse_worker.rs` for queue/replication/metrics processing.
- The worker subsystem includes Redis bus support, replication protocol, health checking, and load balancing abstractions.

### Major feature domains
- `synapse-e2ee/` (+ the root crate's `src/e2ee/` facade): device keys, cross-signing, megolm/olm, verification, secure backup, to-device flows.
- `synapse-federation/` (+ `synapse-web/src/federation/` glue, which depends on the HTTP context): federation transport/auth logic.
- `synapse-services/src/search_service.rs`: supports optional Elasticsearch as well as Postgres-backed search/FTS paths.
- `synapse-services/src/room/`, `src/sync/`, `src/sync_service/`, `src/sliding_sync_service/`: core Matrix room and sync flows.
- The repo also contains non-standard/private-chat extensions described in `README.md`: trusted private chat (`preset=trusted_private_chat`), anti-screenshot signaling (`com.hula.privacy`), and burn-after-read (feature `core-private-chat = friends + burn-after-read`).

## Matrix/Synapse protocol guidance

### Current external baselines
- Treat Matrix Specification latest as the normative protocol source. As of 2026-05-29, the latest published spec is v1.18.
- Treat `element-hq/synapse` as the main behavioral reference for production homeserver tradeoffs. As of 2026-05-29, the latest stable tag observed was `v1.153.0`, with `v1.154.0rc1` as the latest pre-release.
- When changing compatibility-sensitive behavior, record the spec/Synapse version you used in the relevant doc or test name so the baseline is auditable later.

### Protocol declaration discipline
- Be conservative with `/_matrix/client/versions` and `/_matrix/client/v3/capabilities`: only declare stable versions or MSCs that are backed by implementation and tests.
- `synapse-web/src/routes/handlers/versions.rs` currently owns versions, `.well-known`, and capabilities. If changing this surface, prefer typed builders and snapshot/contract tests over ad hoc JSON edits.
- Room-version capability must match actual event/auth behavior. Do not add a room version to `m.room_versions` until create/join/upgrade/redaction/state-resolution behavior is reviewed.
- Keep custom Hula/private-chat extensions namespaced and clearly separated from Matrix stable and MSC identifiers.

### Federation safety rules
- Federation request signing depends on canonical JSON over `method`, `uri`, `origin`, `destination`, and optional `content`. Any change here needs focused tests.
- `Authorization: X-Matrix ...` parsing should be tolerant of normal header formatting, but strict about required fields and signature verification.
- Server key responses and notary query responses have different shapes. Validate `server_name`, `verify_keys`, `old_verify_keys`, `valid_until_ts`, and signatures before caching remote keys.
- Do not weaken origin/user-domain checks in federation membership, device key query/claim, media, or directory endpoints. Prefer returning `M_NOT_FOUND` where the spec expects avoiding room/user existence leaks.

### Upstream Synapse lessons to preserve
- Performance-sensitive experimental behavior should have rollback criteria. Synapse reverted a sliding-sync optimization in v1.153.0rc3 after performance issues.
- Long-running deployments need pruning/background-update paths for device list changes, presence-like state, media quarantine history, and other append-only streams.
- Worker deployments need explicit ownership validation for routes, stream writers, background jobs, and admin operations.
- Canonical JSON, event signatures, and `unsigned` handling are hot and security-sensitive paths; keep tests close to spec vectors and Synapse behavior.

## Repo-specific guidance
- Prefer existing migration/check scripts in `scripts/` and `docker/` over inventing new one-off commands.
- For test expectations and gate definitions, use `TESTING.md` as the current source for what counts as main gate vs extended/manual verification.
- For current capability/status documents, start from the docs index in `README.md` under `docs/synapse-rust/`.
- This repository is broad and heavily modularized; when changing behavior, confirm all three layers affected by the feature: route, service, and storage.
- For the current Matrix/Synapse gap analysis and phased optimization backlog, start from `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`.
- Keep route declarations in sync with `synapse-web/src/routes/route_ledger.rs` and the route manifests. New routes should have manifest entries and duplicate-route coverage.
- If a test requires Postgres setup and hangs in local integration setup, first run the same target with `--no-run` to distinguish compile failures from environment blockers.
- **Format debt is at zero and the CI ratchet is strict (`baseline=0`, both increase AND decrease fail).** Run `cargo fmt --all` before every commit rather than avoiding it — with debt at 0 there is no drift to "hide", and a stale format will fail CI. After large multi-file changes, always verify with `./scripts/check_fmt_ratchet.sh`.
- When a route/format refactor changes line numbers, update `scripts/shell_routes_allowlist.txt` (it is line-number based and drifts with `cargo fmt`) — otherwise placeholder_scan tests fail.
- Snapshot tests (`api_route_ledger` / `api_route_snapshots` / `login_flows_v3`): NEVER run `cargo insta accept` after seeing failures under a narrow feature set — re-run with `--all-features` first; the failures may be feature-set artifacts, and accepting them would bake a false baseline.

## Known pitfalls (cross-referenced from CLAUDE.md §踩过的坑经验总结)
- **Never `unwrap_or_default()` on DB queries in security-relevant paths** (token replay detection, federation prev_events, lazy-load membership) — it silently converts DB errors to "empty/false" defaults. Propagate with `?` or fail-closed.
- **Cache read/write symmetry**: `set_raw` writes L1+L2 async; sync `get_raw` reads L1 only. Cross-instance correctness REQUIRES `get_raw_shared().await`.
- **EDU drop paths** must return `{ dropped: 1, ..Default::default() }`, not `EduProcessResult::default()` (which carries `dropped: 0` and breaks counter aggregation).
- **MSC number discipline**: verify MSC titles against matrix-spec-proposals before wiring routes. MSC4155=Invite filtering, MSC4156=Migrate server_name to via (NOT thread subscriptions — those are private per-user state over account_data and need no EDU federation).
- **`[patch.crates-io]` name iron law**: the patch source crate name must equal the target crate name; there is no rename mechanism. proc-macro crates also cannot re-export another crate's `#[proc_macro]` items — forking is the only path.
- **Batch regex rewrites on Rust attributes**: always dry-run the diff; single-line regexes like `#\[derive\(([^)]+)\)\]` can eat adjacent tokens in multi-line attribute syntax.
- **`tokio::spawn` panics are swallowed** when JoinHandles are dropped; fire-and-forget tasks need `AssertUnwindSafe(..).catch_unwind()` supervision or abort policies. Background loops must wire `CancellationToken` into `tokio::select!` (including reconnect sleeps).

## TDD Workflow

This project follows Red-Green-Refactor TDD. Before implementing any new behavior or fixing a bug, consult:

- **Workflow skill**: `.claude/skills/tdd-rust/SKILL.md` — mandatory Red-Green-Refactor self-check, Cargo command binding, Mock adapter decision tree, insta snapshot rules.
- **Execution checklist**: `.trae/documents/TDD落地执行清单.md` — phased rollout plan (Phase 1–4) with concrete task IDs (P1-x, P2-x, STO-x, FED-x, SYNC-x, P4-x).

### When TDD applies (mandatory)
- Any new feature or route handler behavior
- Bug fixes that touch service/storage logic
- Refactors that change a public response shape

### When TDD does not apply (exceptions)
- Pure formatting / doc-only changes
- Mechanical dependency bumps with no behavior change
- Test infrastructure / fixture-only changes

### Snapshot tests (insta)
- Lock API output shapes for high-frequency routes (`login`, `register`, `sync`, `join`, `profile`).
- Snapshots live under `tests/integration/snapshots/`.
- Dynamic fields (access_token, refresh_token, expires_in, origin_server_ts, user_id suffixes) MUST be redacted via `.redact()` — see SKILL.md §5.
- New snapshots: run `cargo insta test --review` to accept; never commit snapshots you did not review.

### Pre-positioned Mocks
- `synapse-storage::test_mocks::FakeUserStore` / `SharedFakeUserStore` / `seed_locked_users()`
- `synapse-federation::test_mocks::MockFederationClient` (in-memory, pending trait extraction FED-1..4)
- `synapse-services::test_mocks::MockSyncServiceDepsBuilder` (scaffolding pending SYNC-1..6)

### TDD cycle commands (vertical slicing)
```bash
# RED: write one failing test, run it
cargo nextest run -p <crate> <test_name> -P tdd  # or: cargo test -p <crate> <test_name> -- --nocapture
# GREEN: minimal code to pass
# REFACTOR: keep tests green; cargo clippy --all-features --locked -- -D warnings
```
