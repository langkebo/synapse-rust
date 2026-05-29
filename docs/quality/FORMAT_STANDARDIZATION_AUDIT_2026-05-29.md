# Formatting Standardization Audit 2026-05-29

## Scope

This audit establishes a baseline for repository-wide formatting governance in `synapse-rust`.
It focuses on source files and text assets that currently contribute to formatting drift:
`rs`, `md`, `sql`, `sh`, `json`, `yml`, `yaml`, `py`, and `toml`.

## Baseline Inventory

The repository scan identified the following text-heavy footprint before standardization:

| Type | Files |
| --- | ---: |
| `rs` | 605 |
| `md` | 232 |
| `sql` | 222 |
| `sh` | 52 |
| `json` | 41 |
| `yml` | 22 |
| `py` | 21 |
| `yaml` | 13 |
| `toml` | 11 |

## Existing Tooling Before Cleanup

The pre-existing formatting and linting surface was incomplete and partially inconsistent:

- Root `rustfmt.toml` existed and enforced Rust formatting.
- Root `.clippy.toml` existed, but it is lint-only and does not prevent format drift.
- Root `.markdownlint.json` existed, but only selected docs were checked in CI.
- `.vscode/settings.json` enabled Rust format-on-save, but repository-wide editor behavior was not standardized.
- No root `.editorconfig` existed.
- No root `.gitattributes` existed to normalize line endings.
- No `pre-commit` hook configuration existed.
- No dedicated formatting compliance workflow existed in GitHub Actions.
- No `CONTRIBUTING.md` existed to document formatter setup and team expectations.

## Drift Signals Found In Baseline Scan

The initial whole-repository scan found clear style drift across multiple file types:

| Type | Files With Trailing Whitespace | Files Missing Final Newline | Files With Tabs | Files With CRLF |
| --- | ---: | ---: | ---: | ---: |
| `md` | 98 | 4 | 0 | 0 |
| `rs` | 68 | 0 | 0 | 0 |
| `sh` | 16 | 1 | 0 | 0 |
| `sql` | 14 | 32 | 2 | 0 |
| `json` | 0 | 4 | 0 | 0 |
| `py` | 1 | 0 | 0 | 0 |
| `yaml` | 1 | 0 | 0 | 0 |
| `yml` | 1 | 0 | 0 | 0 |

## Key Conflict Points

The most important conflicts discovered during the audit were:

1. Rust had an official formatter config, but cross-editor baseline rules were missing.
2. The repository-level Rust width standard is `120` in `rustfmt.toml`, while the checked-in VS Code Rust ruler is still `100`.
3. Text-file hygiene rules such as `LF`, `final newline`, and trailing whitespace trimming were not centrally enforced.
4. Python and shell files had no mandatory formatting gate even though they are present in operational scripts.
5. Markdown quality checks existed, but they were not part of a unified formatting workflow.

## Standardized Toolchain Chosen

Because this repository is Rust-dominant with operational Python and shell utilities, the standardized toolchain is:

- Rust: `rustfmt`
- Python: `ruff format`
- Shell: `shfmt`
- Cross-file hygiene: `pre-commit-hooks` + `.editorconfig` + `.gitattributes`
- Documentation quality: existing `markdownlint` workflow

No `ESLint` or `Prettier` rollout was introduced because the repository does not contain a maintained frontend codebase that would justify a Node-first formatting stack.

## Changes Introduced By This Standardization Pass

This governance pass adds the missing repository-level controls:

- root `.editorconfig`
- root `.gitattributes`
- root `.pre-commit-config.yaml`
- `scripts/quality/format_audit.py`
- `scripts/quality/format_check.sh`
- `scripts/quality/format_write.sh`
- `Makefile` entrypoints for install, format, check, and audit
- `CONTRIBUTING.md`
- GitHub Actions workflow: `format-governance.yml`

## Full-Repository Reformat Strategy

The repository should still perform a one-time full-tree formatting pass, but that change must remain isolated:

1. create a dedicated branch for formatting-only changes;
2. run `make format`;
3. review the diff with extra care for SQL migration files and Markdown hard line breaks;
4. submit the result as a standalone PR with no functional changes mixed in.

## Three-Cycle Verification Plan

To verify that the fix is durable, track the next three delivery cycles and confirm:

1. every PR passes `Format Governance` on the first or second attempt;
2. no new trailing-whitespace or final-newline regressions appear in audit output;
3. no new subdirectory formatting config files are introduced;
4. all developers use the root `Makefile` or installed `pre-commit` hooks locally.

The follow-up implementation is now backed by:

- `make format-cycle CYCLE_LABEL=<cycle-name>` to rerun compliance checks and refresh the rolling report;
- `scripts/quality/format_cycle_report.py` to keep the latest three cycle entries in `docs/quality/FORMAT_DRIFT_TRACKING.md`;
- GitHub Actions workflow `format-drift-tracking.yml` for scheduled/manual checkpoint artifacts.
