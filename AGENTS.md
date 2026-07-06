# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Common commands

### Local Rust development
- Build: `cargo build --locked`
- Run server: `SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --release`
- Run worker binary: `cargo run --bin synapse_worker`
- Format check: `cargo fmt --all -- --check`
- Clippy: `cargo clippy --all-features --locked -- -D warnings`
- Doc tests: `cargo test --doc --locked`
- Full test suite: `cargo test --all-features --locked -- --test-threads=4`
- CI-equivalent Rust test entrypoint: `TEST_THREADS=4 TEST_RETRIES=2 bash scripts/run_ci_tests.sh`
- If `cargo-nextest` is installed, `scripts/run_ci_tests.sh` uses it automatically; otherwise it falls back to `cargo test` with retries.

### Running specific tests
- Unit test target: `cargo test --test unit`
- Integration test target: `cargo test --features test-utils --test integration`
- E2E target: `cargo test --test e2e`
- Performance manual target: `cargo test --features performance-tests --test performance_manual -- --nocapture`
- Run one named integration test: `cargo test --features test-utils --test integration <test_name> -- --nocapture`
- Compile one integration test target without running DB setup: `cargo test --features test-utils --test integration <test_name> --no-run`
- Run one unit test from the unit target: `cargo test --test unit <test_name> -- --exact --nocapture`
- Run one library unit test by substring: `cargo test --lib <test_name> -- --nocapture`

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
- Prefer `server.server_name`/`ServerConfig::get_server_name()` for Matrix identity. `server.name` exists for compatibility but can differ from the public Matrix server name in delegated or reverse-proxy deployments.
- Federation config has its own `federation.server_name`; when validating local identity, account for all locally accepted names rather than hard-coding one config field.

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

## Matrix/Synapse protocol guidance

### Current external baselines
- Treat Matrix Specification latest as the normative protocol source. As of 2026-05-29, the latest published spec is v1.18.
- Treat `element-hq/synapse` as the main behavioral reference for production homeserver tradeoffs. As of 2026-05-29, the latest stable tag observed was `v1.153.0`, with `v1.154.0rc1` as the latest pre-release.
- When changing compatibility-sensitive behavior, record the spec/Synapse version you used in the relevant doc or test name so the baseline is auditable later.

### Protocol declaration discipline
- Be conservative with `/_matrix/client/versions` and `/_matrix/client/v3/capabilities`: only declare stable versions or MSCs that are backed by implementation and tests.
- `src/web/routes/handlers/versions.rs` currently owns versions, `.well-known`, and capabilities. If changing this surface, prefer typed builders and snapshot/contract tests over ad hoc JSON edits.
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
- For the current Matrix/Synapse gap analysis and phased optimization backlog, start from `docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md`.
- Keep route declarations in sync with `src/web/routes/route_ledger.rs` and the route manifests. New routes should have manifest entries and duplicate-route coverage.
- If a test requires Postgres setup and hangs in local integration setup, first run the same target with `--no-run` to distinguish compile failures from environment blockers.
- The repository may have pre-existing formatting drift. Avoid broad `cargo fmt --all` rewrites unless the task is explicitly formatting cleanup; prefer focused formatting/checks for touched files.

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
