# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
- Integration test target: `cargo test --test integration`
- E2E target: `cargo test --test e2e`
- Performance manual target: `cargo test --features performance-tests --test performance_manual -- --nocapture`
- Run one named test: `cargo test --test integration <test_name> -- --exact --nocapture`
- Run one unit test from the unit target: `cargo test --test unit <test_name> -- --exact --nocapture`

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
