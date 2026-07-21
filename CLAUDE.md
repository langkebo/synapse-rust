# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common commands

### Local Rust development
- Build: `cargo build --locked`
- Run server: `SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --release`
- Run worker binary: `cargo run --bin synapse_worker`
- Format check: `cargo fmt --all -- --check`
- Clippy: `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings`
- Doc tests: `cargo test --doc --locked`

- Enable local git hooks: `git config core.hooksPath .githooks` (pre-commit: cargo audit advisory, pre-push: cargo deny advisories blocking)

### Running tests

- **Full suite (all lib + unit tests, no DB):**
  `cargo nt --lib --test unit`
- **Lib tests only:** `cargo nt --lib`
- **Unit test target only:** `cargo nt --test unit`
- **Single named test:**
  `cargo nt --test unit <test_name>`
- **Integration tests (requires PostgreSQL):**
  `cargo nt --features privacy-ext,voice-extended,voip-tracking,beacons,server-notifications --test integration`
- **Full CI suite:** `bash scripts/run_ci_tests.sh`
- **Clippy:** `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings`

**Why `--profile test` is required:** Lib tests in `synapse-services` import
`test_mocks` modules from sibling crates (`synapse-storage`, `synapse-e2ee`,
`synapse-federation`). These modules are gated on
`#[cfg(any(test, feature = "test-utils"))]`. Rust's `#[cfg(test)]` does NOT
propagate to dependency crates — so without `--features test-utils`, the
`test_mocks` modules are missing at compile time and you get 119 E0432/E0433
errors.

The `test` nextest profile injects `test-utils` automatically.

### Benchmarks and coverage
- API benchmark compile/run path: `cargo bench --bench performance_api_benchmarks --no-run`
- Federation benchmark compile/run path: `cargo bench --bench performance_federation_benchmarks --no-run`
- Coverage (if installed): `cargo tarpaulin --output-dir coverage/ --html`

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
- `src/server.rs` is the main composition root. It creates the Postgres pool, runs schema health checks, wires Redis/in-memory cache, builds `ServiceContainer`, configures rate-limit state, and assembles the Axum router.
- The server exposes both client and federation listeners from the same application state.

### Core layering
- `src/web/`: HTTP boundary. Axum routes, extractors, middleware, validators, and Matrix-compatible endpoint assembly.
- `src/services/`: business logic layer. This is where feature behavior lives.
- `src/storage/`: persistence layer over PostgreSQL (sqlx), plus schema/health/performance helpers.
- `src/cache/` and service-local cache modules: Redis-backed cache when enabled, otherwise in-memory fallback.
- `src/common/`: shared config, logging, security, rate limit, task queue, and utility code.

The codebase generally follows `route -> service -> storage`, with `AppState`/`ServiceContainer` carrying shared dependencies.

### Router organization
- `src/web/routes/assembly.rs` is the top-level router assembly point.
- It merges many feature routers under Matrix-compatible prefixes such as `/_matrix/client/*`, `/_matrix/federation/*`, and admin/auxiliary endpoints.
- Middleware layering is centralized here: CORS, security headers, compression, CSRF, and rate limiting.
- Route implementation is split by domain under `src/web/routes/` and `src/web/routes/handlers/`.

### Dependency wiring
- `src/services/container.rs` is the main dependency graph for application features.
- It constructs storages and services for auth, rooms, sync, sliding sync, E2EE, federation helpers, media, push, moderation, retention, feature flags, worker integration, and more.
- If you need to understand how a feature is actually enabled end-to-end, start at `ServiceContainer::new(...)`, then trace the relevant router and storage.

### Storage and schema model
- Postgres is the primary source of truth.
- `src/storage/mod.rs` re-exports many domain-specific storages; most features have a corresponding storage module.
- `src/storage/schema_health_check.rs` is part of startup validation. Missing critical tables/columns fail startup.
- Runtime DB initialization is intentionally not the default path. The expected migration flow is externalized through `docker/db_migrate.sh`; server startup only performs schema health checks unless `SYNAPSE_ENABLE_RUNTIME_DB_INIT` is explicitly enabled and `SYNAPSE_SKIP_DB_INIT` is not set.

### Configuration model
- Config types live in `src/common/config/`.
- Main config is file-based (`SYNAPSE_CONFIG_PATH`, default `homeserver.yaml`) with `SYNAPSE_` environment variable overrides using `__` for nesting.
- Docker uses `docker/config/homeserver.yaml` and mounts `docker/config/rate_limit.yaml`.
- Search must exist structurally in config; when Elasticsearch is not used it should still be explicitly disabled.

### Caching and async/background work
- Redis is optional but first-class. When enabled, the server uses Redis-backed cache and task queue infrastructure; otherwise cache falls back to local memory.
- `ScheduledTasks` and task metrics are initialized in `src/server.rs`.
- Worker-related code lives under `src/worker/`, with an additional binary in `src/bin/synapse_worker.rs` for queue/replication/metrics processing.
- The worker subsystem includes Redis bus support, replication protocol, health checking, and load balancing abstractions.

### Major feature domains
- `src/e2ee/`: device keys, cross-signing, megolm/olm, verification, secure backup, to-device flows.
- `src/federation/`: federation transport/auth logic plus friend-federation extensions.
- `src/services/search_service.rs`: supports optional Elasticsearch as well as Postgres-backed search/FTS paths.
- `src/services/room_service.rs`, `sync_service.rs`, `sliding_sync_service.rs`: core Matrix room and sync flows.
- The repo also contains non-standard/private-chat extensions described in `README.md`, including trusted private chat, anti-screenshot signaling (`com.hula.privacy`), and burn-after-read behavior.

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
Baseline files: docs/audit/00_test_baseline.log, docs/audit/00_clippy_baseline.log,
docs/audit/05_performance_baseline.log, docs/audit/11_performance_after.log.

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
