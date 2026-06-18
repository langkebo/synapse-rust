# Contributing

## Formatting Baseline

This repository is Rust-first and uses a single root-level formatting policy.
All source and documentation changes must follow the rules below before review.

- Rust code uses `rustfmt` with the root `rustfmt.toml`.
- Python utilities use `ruff format`.
- Shell scripts use `shfmt`.
- Cross-file hygiene uses `.editorconfig`, `.gitattributes`, and `.pre-commit-config.yaml`.
- Markdown quality continues to use the existing `.markdownlint.json` gate.

## One-Time Setup

Install the local formatting toolchain once per machine.

```bash
rustup component add rustfmt
python3 -m pip install --user pre-commit ruff
brew install shfmt
pre-commit install --hook-type pre-commit --hook-type pre-push
```

If you do not use Homebrew, install `shfmt` with your platform package manager.

## Local Commands

Use the root `Makefile` so every developer runs the same entrypoints.

```bash
make format-install
make format
make format-check
make format-audit
make format-cycle CYCLE_LABEL=cycle-1
```

- `make format` applies the repository formatters and then verifies the tree is clean.
- `make format-check` runs the same compliance checks used by CI.
- `make format-audit` refreshes the drift audit report.
- `make format-cycle` reruns the compliance gate and refreshes the rolling three-cycle tracking report.

## Commit Policy

- Do not mix large-scale formatting rewrites with feature changes.
- If the repository needs a full-tree reformat, submit it as a dedicated PR.
- Let `pre-commit` block commits that introduce formatting regressions.

## CI Policy

- The `Format Governance` GitHub Actions workflow runs on pushes and pull requests.
- Any formatting violation blocks the workflow and shows the failing files.
- Contributors should run `make format-check` locally before opening a PR.

## Follow-Up Verification

After the baseline rollout, maintainers should review three consecutive delivery cycles and confirm:

- new PRs pass `Format Governance` without manual cleanup;
- no new CRLF, trailing whitespace, or final-newline drift appears;
- Rust, Python, and shell files continue to be formatted through the shared entrypoints.
- run `make format-cycle CYCLE_LABEL=<cycle-name>` at each release/sprint checkpoint so `docs/quality/FORMAT_DRIFT_TRACKING.md` keeps the latest three cycle snapshots.
- use the scheduled `Format Drift Tracking` workflow artifact when maintainers want an unattended checkpoint from `main`.

## Architecture Overview

synapse-rust follows a layered architecture: `route → service → storage`.

- **`src/web/`** — HTTP boundary (Axum routes, extractors, middleware)
- **`src/services/`** — Business logic layer (`ServiceContainer` is the composition root)
- **`src/storage/`** — Persistence layer (PostgreSQL via sqlx, thin facades to `synapse-storage`)
- **`synapse-services/`** — Canonical service implementations
- **`synapse-storage/`** — Canonical storage implementations
- **`synapse-e2ee/`** — End-to-end encryption (vodozemac-based Megolm/Olm)
- **`synapse-federation/`** — Federation protocol
- **`synapse-common/`** — Shared config, crypto, error types
- **`synapse-cache/`** — Redis-backed cache with in-memory fallback

When changing behavior, confirm all three layers affected by the feature: route, service, and storage.

## Development Workflow

### Build & Run

```bash
cargo build --locked
SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --release
```

### Testing

| Type | Command |
|------|---------|
| Unit | `cargo test --test unit` |
| Integration | `cargo test --features test-utils --test integration` |
| E2E | `cargo test --test e2e` |
| Lib | `cargo test --lib` |
| Doc tests | `cargo test --doc --locked` |
| Full suite | `cargo test --all-features --locked -- --test-threads=4` |

### Code Quality Gates

Before opening a PR, ensure all gates pass:

```bash
cargo fmt --all -- --check
cargo clippy --all-features --locked -- -D warnings
cargo check --workspace --all-features --locked
bash scripts/ci/check_sqlx_offline_cache.sh
python3 scripts/ci/check_root_canonical_ledger.py
```

## PR Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-features --locked -- -D warnings` passes
- [ ] `cargo test --lib` passes (0 failures)
- [ ] `cargo test --test unit` passes
- [ ] No new `unwrap()` in production code (use `?` or `unwrap_or_default()`)
- [ ] No new `todo!()` or `unimplemented!()`
- [ ] SQL queries use parameterized bindings (no `format!` with SQL)
- [ ] New routes have entries in `route_ledger.rs`
- [ ] New migrations follow `YYYYMMDDHHMMSS_description.sql` naming
- [ ] `.sqlx/` cache is updated if SQL queries changed
- [ ] Documentation links are valid (`bash scripts/ci/check_doc_links.sh`)

## Database Migrations

- Migration source of truth: `docker/db_migrate.sh`
- Apply locally: `bash docker/db_migrate.sh migrate`
- Validate: `bash docker/db_migrate.sh validate`
- Never use `sqlx migrate` directly against root `migrations/`

## Configuration

- Config file: `homeserver.yaml` (path via `SYNAPSE_CONFIG_PATH`)
- Env overrides: `SYNAPSE_` prefix with `__` for nesting (e.g., `SYNAPSE_DATABASE__HOST`)
- Placeholder syntax: `${VAR}`, `${VAR:-default}`, `${VAR:?error}`
