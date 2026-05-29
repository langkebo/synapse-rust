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
