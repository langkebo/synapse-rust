# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common commands

### Local Rust development
- Build: `cargo build --locked`
- Run server: `SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --release`
- Run worker binary: `cargo run --bin synapse_worker`
- Format check: `cargo fmt --all -- --check`
- Clippy (CI runs both matrix entries): `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils [--all-features] --locked -- -D warnings`
- Doc tests: `cargo test --doc --locked`

- Enable local git hooks: `git config core.hooksPath .githooks` — **未启用时 `.githooks/` 里的 hook 不会执行**（默认活动目录是 `.git/hooks`，只有 `*.sample`）。pre-commit 现在**阻断** `./scripts/check_fmt_ratchet.sh` 失败（即 CI 同一门禁，整树检查），clippy 阶段需 `SYNAPSE_PRECOMMIT_CLIPPY=1` 才启用，cargo-audit 仍为 advisory；pre-push **阻断** CI 同款 clippy（`--workspace --all-targets --features test-utils --all-features`，因而能捕获"测试代码没在 `--all-features` 下编译过"这类缺陷）并保留原有的 cargo deny advisories 阻断

### Running tests

- **Full suite (all lib + unit tests, no DB):**
  `cargo nextest run --workspace --lib --all-features --locked --test-threads 4` then
  `cargo nextest run --test unit --features test-utils --locked --test-threads 4`
  ⚠️ `--workspace` 不可省：不带它时 lib 批次只覆盖 root package（root `Cargo.toml` 无 `default-members`），
  8 个 member crate 的 lib 单测不会执行（2026-09-22 实测漏 6139 个用例，判据见
  `docs/audit/FULL_SUITE_ISOLATION_VERIFY_2026-09-22.md` §5）。`cargo nt --lib` 同样只覆盖 root。
- **Lib tests only:** `cargo nt --lib`
- **Unit test target only:** `cargo nt --test unit`
- **Single named test:**
  `cargo nt --test unit <test_name>`
- **Integration tests (requires PostgreSQL):**
  `cargo nt --features privacy-ext,voice-extended,voip-tracking,beacons,server-notifications --test integration`
- **Single named integration test:** `cargo nt --test integration <test_name>`
- **Local CI run (⚠️ not the CI entrypoint — no workflow calls it):** `bash scripts/ci_backend_validation.sh` (runs `ci.yml`'s lib/unit/integration nextest batches verbatim; the old `scripts/run_ci_tests.sh` replica was deleted under sweep A13)
- **Clippy (CI runs both matrix entries):** `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils [--all-features] --locked -- -D warnings`
- **E2E tests:** `cargo nextest run --test e2e`
- **Performance manual tests:** `cargo nextest run --features performance-tests --test performance_manual -- --nocapture`

**Why the `test` nextest profile is required:** Lib tests in `synapse-services` import
`test_mocks` modules from sibling crates (`synapse-storage`, `synapse-e2ee`,
`synapse-federation`). These modules are gated on
`#[cfg(any(test, feature = "test-utils"))]`. Rust's `#[cfg(test)]` does NOT
propagate to dependency crates — so without `--features test-utils`, the
`test_mocks` modules are missing at compile time and you get 119 E0432/E0433
errors.

The `test` nextest profile injects `test-utils` automatically. `cargo nt` is
an alias for `cargo nextest run --profile test --features test-utils`.

### Benchmarks and coverage
- API benchmark compile/run path: `cargo bench --bench performance_api_benchmarks --no-run`
- Federation benchmark compile/run path: `cargo bench --bench performance_federation_benchmarks --no-run`
- Coverage: CI uses `cargo llvm-cov --workspace` (tarpaulin was replaced). Local end-to-end run: `bash scripts/ci/run_coverage.sh`（CI 调用的**同一条命令**；本机 ~35–50 min）。

### Database and migrations
- Migration source of truth: `docker/db_migrate.sh`
- Apply migrations locally: `bash docker/db_migrate.sh migrate`
- Validate migrations/schema locally: `bash docker/db_migrate.sh validate`
- Migration verification helpers live under `scripts/` and `migrations/`; prefer existing scripts over ad hoc SQL.

### Docker workflow
- Start full stack: `cd docker && docker compose up -d --build`
- Validate containerized migrations: `cd docker && docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate`
- Validate schema in container: `cd docker && docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate`
- CI-like local validation: `bash scripts/ci_backend_validation.sh`

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
- It constructs storages and services for auth, rooms, sync, sliding sync, E2EE, federation helpers, media, push, retention, feature flags, worker integration, and more.
  (The client "report" routes in `synapse-web/src/routes/moderation.rs` and member
  management in `synapse-services/src/room/membership/moderation.rs` are unrelated to
  the removed moderation rule-engine domain, which had its own storage/service here.)
- If you need to understand how a feature is actually enabled end-to-end, start at `ServiceContainer::new(...)`, then trace the relevant router and storage.

### Storage and schema model
- Postgres is the primary source of truth.
- `synapse-storage/src/lib.rs` re-exports domain-specific storages; most features have a corresponding storage module.
- `synapse-storage/src/schema_health_check.rs` is part of startup validation. Missing critical tables/columns fail startup.
- Runtime DB initialization is intentionally not the default path. The expected migration flow is externalized through `docker/db_migrate.sh`; server startup only performs schema health checks unless `SYNAPSE_ENABLE_RUNTIME_DB_INIT` is explicitly enabled and `SYNAPSE_SKIP_DB_INIT` is not set.

### Configuration model
- Config types live in `synapse-common/src/config/` (the root crate's `src/common/config/` is a thin re-export).
- Main config is file-based (`SYNAPSE_CONFIG_PATH`, default `homeserver.yaml`) with `SYNAPSE_` environment variable overrides using `__` for nesting.
- Docker uses `docker/config/homeserver.yaml` and mounts `docker/config/rate_limit.yaml` — `docker/config/` is the single config source for both compose stacks and for the image; there is no second copy under `docker/deploy/`.
- Search must exist structurally in config; when Elasticsearch is not used it should still be explicitly disabled.

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

## 项目状态与反冗余铁律（未发布项目，先读这一节）

**权威来源**：以下规则在 `AGENTS.md` §"项目状态与反冗余铁律"完整定义并维护，
**修改只改 AGENTS.md，这里只保留同步摘要**，避免双源漂移。

**项目状态：未发布、无外部用户、无生产数据。因此不存在向后兼容义务。**

1. **禁止兼容残留。** 不得为"向后兼容"保留 `#[deprecated]` 项、旧路径别名、
   双实现、feature 开关式的死代码、或"先留着以后可能有人用"的中间态。
   改动直接替换原实现。**判据**：如果某符号/分支/配置的唯一存在理由是
   "兼容旧行为"，就删掉它。
2. **同一职责只允许一份实现。** 基础设施出现第二份实现即视为缺陷，新增前
   先搜索是否已有可复用实现（`synapse-common` 是首选位置）。
3. **测试/bench 专用依赖必须放 `[dev-dependencies]`。** 不得进 `[dependencies]`。
4. **构建产物与派生缓存不入库。** `artifacts/`、`target/`、SQLx 派生缓存一律
   gitignore；不得用 `git add -f` 绕过。
5. **`cargo metadata` 必须始终可用。** 任何 vendored 或独立 crate 必须在根
   `Cargo.toml` 的 `[workspace] members` 或 `exclude` 中显式声明。
6. **薄壳禁止。** 根 crate 的 `src/{services,storage,common}/mod.rs` 只允许
   re-export，且必须有实际使用者。
7. **消除冗余优先于绕过问题。** 遇到并发/共享状态类缺陷，先问"能否从结构上
   消除共享"（如 per-test schema 隔离），而不是加锁/串行化/重试绕过。
8. **门禁必须自证能变红。** 任何"检查类"门禁在新增或修改后，必须用故意制造的
   违规证明它真的会失败。看到"长期 0 违规 / 长期全绿"的门禁，优先怀疑它
   没在工作，而不是相信代码很干净。

**推论（结合 ledger 契约链教训）**：
- 后端 `SCHEMA_VERSION` 一旦变更，必须同步 SDK 侧 `LEDGER_SCHEMA_VERSION` pin
  与两条车道 fixture（见 `docs/audit/LEDGER_CONTRACT_ISSUES_2026-09-13.md`）。
- 新增的路由元数据字段（如 `module`/`status`）若没有下游消费方，属于冗余，
  应按第 1 条删除或补齐消费方，不得以"保留备用"为由滞留。

## Repo-specific guidance
- Prefer existing migration/check scripts in `scripts/` and `docker/` over inventing new one-off commands.
- For test expectations and gate definitions, use `TESTING.md` as the current source for what counts as main gate vs extended/manual verification.
- For current capability/status documents, start from the docs index in `README.md` under `docs/synapse-rust/`.
- This repository is broad and heavily modularized; when changing behavior, confirm all three layers affected by the feature: route, service, and storage.

## Skill routing

When the user's request matches an available skill, invoke it via the Skill tool. When in doubt, invoke the skill.

Key routing rules:
- Product ideas/brainstorming → invoke /office-hours
- Strategy/scope → invoke /plan-ceo-review
- Architecture → invoke /plan-eng-review
- Design system/plan review → invoke /design-consultation or /plan-design-review
- Full review pipeline → invoke /autoplan
- Bugs/errors → invoke /investigate
- QA/testing site behavior → invoke /qa or /qa-only
- Code review/diff check → invoke /review
- Visual polish → invoke /design-review
- Ship/deploy/PR → invoke /ship or /land-and-deploy
- Save progress → invoke /context-save
- Resume context → invoke /context-restore
- Author a backlog-ready spec/issue → invoke /spec
- TDD/test-first development → invoke /tdd-rust

## gstack
Use /browse from gstack for all web browsing. Never use mcp__claude-in-chrome__* tools.
Available skills: /office-hours, /plan-ceo-review, /plan-eng-review, /plan-design-review,
/design-consultation, /review, /ship, /land-and-deploy, /canary, /benchmark, /browse,
/qa, /qa-only, /design-review, /setup-browser-cookies, /setup-deploy, /retro,
/investigate, /document-release, /codex, /cso, /autoplan, /careful, /freeze, /guard,
/unfreeze, /gstack-upgrade

## superpowers
Available commands: /superpowers:brainstorm, /superpowers:write-plan, /superpowers:execute-plan.
Skills auto-activate on context: brainstorming, writing-plans, executing-plans,
test-driven-development, systematic-debugging, subagent-driven-development,
verification-before-completion, using-git-worktrees, finishing-a-development-branch,
requesting-code-review, receiving-code-review, dispatching-parallel-agents.

## audit workspace convention
All review reports go to docs/audit/NN_<name>.md (NN = 01..13).
Audit branch naming: optimization/audit-YYYY-MM.
Baseline files (ratchet inputs that actually exist):
`scripts/.fmt-baseline`, `scripts/.missing-docs-baseline`, `scripts/ci/trait_count_baseline`,
`scripts/ci/sqlx_dynamic_ratio_baseline`, `scripts/ci/geiger_baseline.json`,
`scripts/ci/coverage_baseline.json`（per-file 覆盖率棘轮；2026-09-19 从 `artifacts/` 移入 —— `artifacts/*` 被 gitignore 而只有它一个例外，属反冗余铁律 4 的破例）。

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
- CI asserts snapshots with `INSTA_UPDATE=no` plus a committed/leftover `.snap.new` check, and does **not** use cargo-insta at all (its CLI semantics changed twice) — see the `Snapshot gate` step in `ci.yml`.

### Pre-positioned Mocks
- `synapse-storage::test_mocks`: `FakeUserStore` / `SharedFakeUserStore` / `seed_locked_users()`, `InMemoryEventStore`, `InMemoryRoomStore`, `InMemoryMemberStore`
- `synapse-storage`: `EventStoreApi`, `RoomStoreApi`, `MemberStoreApi`, `PresenceStoreApi` traits with `Arc<dyn Trait>` injection
- `synapse-federation::test_mocks::MockFederationClient` — implements `FederationClientApi`, seed responses via `seed_*()` methods
- `synapse-federation::client_api::FederationClientApi` — trait seam, inject `Arc<dyn FederationClientApi>`
- `synapse-services::test_mocks`: `MockSyncServiceDepsBuilder`, `FakeAuth` (configurable `validate_token`), `TestSyncContext`
- `synapse-services::auth::Auth` — already a trait, mock via `FakeAuth`

### TDD cycle commands (vertical slicing)
```bash
# RED: write one failing test, run it
cargo nextest run -p <crate> <test_name> -P tdd  # or: cargo test -p <crate> <test_name> -- --nocapture
# GREEN: minimal code to pass
# REFACTOR: keep tests green; cargo clippy --all-features --locked -- -D warnings
```

---

## 🚀 踩过的坑经验总结

### 1. 测试门禁陷阱

**❌ 假绿/假失败陷阱**
- `cargo test --doc --locked` 在根 crate 运行 0 个测试 → 永远 PASS
- 集成测试 feature 集不匹配：`/login` 快照测试、路由 ledger 快照受 `cas-sso`/`saml-sso`/`builtin-oidc` 等 feature 影响
- **解法**：永远用 `--all-features` 或匹配 CI 口径的 feature 列表

**❌ cargo nt 别名坑**
- `cargo nt` 是 `.cargo/config.toml` 定义的 alias，等价于 `nextest run --profile test --features test-utils`
- nextest 0.9.140 的 `[profile.test]` 中的 `features` 被静默忽略，`cargo nt --test integration` 编译会失败

**❌ fmt 棘轮陷阱**
- CI 使用 `scripts/check_fmt_ratchet.sh` 严苛模式：current=0, baseline=0
- `use_small_heuristics = "Max"` 导致 rustfmt 倾向压缩，频繁与手写风格冲突

### 2. 代码实现陷阱

**❌ unwrap_or_default 吞错**
```rust
// BAD: DB 错误被吞掉，可能导致脏数据
refresh_token.is_active().await.unwrap_or_default()

// GOOD: 错误传播或 Fail-closed
refresh_token.is_active().await?
// 或
refresh_token.is_active().await.map_err(|_| ServiceError::DatabaseError)?
```

**❌ 计数器不一致**
- `EduProcessResult::default()` 返回 `dropped:0`，但计数器已递增
- **解法**：统一返回 `{ dropped: 1, ..Default::default() }` 对于 drop 路径

**❌ 测试 mock 导入陷阱**
- `synapse-storage::test_mocks` 被 `#[cfg(any(test, feature="test-utils"))]` 门控
- 跨 crate 测试用到 mock 必须 `--features test-utils`，否则 E0432/E0433

**❌ 行号型 allowlist 与 cargo fmt 冲突**
- `scripts/shell_routes_allowlist.txt` 用「文件路径 + 行号」做静态扫描豁免，任何 `cargo fmt` 都会让行号漂移导致豁免失效
- 新代码路径产生 `Ok(empty_json())` 时记得补录 allowlist（历史上 MSC4204/4267 改动漏录 3 行直接红 CI）
- 理想方案：改用 `git grep` 标记或函数级 match 模式（待办）

### 3. Federation EDU 语义漂移

**⚠️ MSC 编号语义注意**：
- MSC4155 = **Invite filtering**（邀请过滤）
- MSC4156 = **Migrate server_name to via**（server_name → via）
- 都与「线程订阅」无关，路线图文档中表述为「线程订阅跨服务器同步」是语义漂移

**线程订阅归类**：
- `thread_subscriptions` 是用户私有态（account_data），不跨服务器同步
- 不需要 `EduType::ThreadSubscription`、`broadcast_thread_subscription_edu`
- 裁定：**线程订阅不实现 EDU 联邦**，用 device-list stream 唤醒本人设备

### 4. 数据库与迁移陷阱

**❌ Postgres 错误码误判**
- `schema "x" does not exist` → SQLSTATE `3F000` 不是 `42P01`
- `CREATE OR REPLACE FUNCTION` 不能改参数名 → 需先 `DROP FUNCTION IF EXISTS`

**❌ 迁移双副本漂移（已于 `2b16dc3c` 根治）**
- 历史坑：`docker/deploy/migrations/` 是被 git 跟踪的手工同步副本（191 文件），与权威源 `migrations/` 严重漂移（详见 `migrations/README.md`（副本独有 131 = 非 `archive/` 82［其中正向 42］+ `archive/` 49）；权威独有 13 个迁移、同名文件内容不一致）→ 走 deploy 路径的全新部署会**静默跳过这 13 个迁移**
- 现状：死副本已删除，`migrations/` 为**单一真相源**（deploy 经 docker-compose 挂载它）
- **规则**：新增迁移只写 `migrations/`，不要再创建任何副本目录；旧文档中提到 `docker/deploy/migrations/` 的均属过时信息

**❌ 测试 schema 残留**
- 单个 integration 运行产出数百个 test schema（255 表/个）
- 1363 个残留 schema → 5432 端口 5432 实例爆炸
- **解法**：`scripts/cleanup_test_schemas.sh` 但未自动调用

### 5. 依赖治理陷阱

**❌ `[patch.crates-io]` 需求铁律**
- source crate name 必须等于 target crate name
- `patch = { package = "paste", ... }` 形式无效，name 必须为 `paste`

**❌ proc-macro re-export 限制**
- Rust 不允许 proc-macro crate export 非 `#[proc_macro]` 条目
- `pub use pastey::paste` 因 `#[proc_macro]` 属性丢失会失败

**❌ 多行 derive 正则误伤**
- Python regex `re.sub(r'#\[derive\(([^)]+)\)\]', ...)` 误吞 `)]`
- **解法**：语法化处理或永远 dry-run 看 diff

### 6. 客户端登录陷阱

**❌ 登录后不可预先 validate_token**
- 触发 `cache.set_user_active + cache.set_token` 污染缓存
- 第二次 validate 走 cache 分支绕过 DB 的 `is_deactivated` 检查
- **解法**：登录成功后的首次 validate 需走 DB 分支，或显式清理缓存后再从 DB 获取有效状态

### 7. SSRF 防御细节

**❌ IPv6 URL 处理**
- `url::Url::host_str()` 对 IPv6 返回带 `[]` 形式（`"[::1]"`），需 strip 后 parse
- `Ipv6Addr::is_loopback/is_unspecified` 已含 IPv4-mapped IPv6 (`::ffff:127.0.0.1`)
- IPv6 ULA (`fc00::/7`) 与 link-local (`fe80::/10`) 需手动判定，stdlib 无现成方法

### 8. 可观测性陷阱

**❌ request-id 链路追踪**
- `RequestIdPropagationLayer` 已注册但无代码写入 span extensions
- **解法**：`info_span!("http_request", request_id = %request_id)` + layer 从 `Attributes` 读字段写 extensions

### 9. SQL 迁移与脚本陷阱

**❌ 函数参数名变更**
- `CREATE OR REPLACE FUNCTION` 不能改参数名（参数名属函数签名的一部分，PG 会拒绝）
- 幂等迁移应先 `DROP FUNCTION IF EXISTS ...(旧签名)`，再 `CREATE OR REPLACE`

**❌ dollar-quoting 块拆分误伤**
- 脚本里 `split(';')` 会把 `$$...$$` 函数/DO 块按内部 `;` 切碎（导致触发器函数从未创建）
- 需字符级扫描正确处理 `'...'`、`"..."`、`$$/$tag$` dollar-quoting、`--`/`/* */` 注释后才能安全切分

### 10. 集成测试并发敏感

**❌ 同一提交测试结果漂移**
- `--test-threads 12` 下：1408 passed + 12 failed + 7 timed out
- `--test-threads 1` 下（同负载）：13/13 100% 通过
- **根因**：并发资源争用导致超时，与代码无关
- **解法**：CI 中 `api_media_routes_tests`、`protocol_compliance_tests`、`database_integrity_tests` 同样需要限并发

### 11. 关键命令速查

| 场景 | 推荐命令 |
|------|----------|
| fmt 检查 | `./scripts/check_fmt_ratchet.sh` |
| 集成测试 | `cargo nextest run --profile ci --all-features --test integration --test-threads 1` |
| 同步测试 | `cargo nextest run -p <crate> <test_name> -P tdd` |
| DB 迁移 | `DATABASE_URL=... bash docker/db_migrate.sh migrate` |
| 覆盖率 | `bash scripts/ci/run_coverage.sh`（唯一实现，CI 同款） |
| 完整 CI 本地入口（**非** CI 触发；逐字执行 ci.yml 的 lib/unit/integration 批次） | `bash scripts/ci_backend_validation.sh` |

### 记住：每次提交前必检
```bash
git status                           # 核对工作树
cargo fmt --all -- --check          # 格式
cargo clippy --all-features -- -D warnings  # lint
cargo test --all-features           # 功能

