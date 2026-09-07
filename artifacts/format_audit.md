# Format Drift Audit

- Generated at: `2026-09-02 07:51:56Z`
- Repository: `/Users/ljf/Desktop/hu_ts/synapse-rust`

## File Distribution

| Extension | Files |
| --- | ---: |
| `.rs` | 1989 |
| `.json` | 1652 |
| `.md` | 789 |
| `.sql` | 377 |
| `.sh` | 269 |
| `.py` | 89 |
| `.yml` | 54 |
| `.toml` | 37 |
| `.yaml` | 37 |

## Formatting Drift Signals

| Extension | Trailing WS | CRLF | Tabs | Missing Final Newline |
| --- | ---: | ---: | ---: | ---: |
| `.json` | 0 | 0 | 0 | 1347 |
| `.md` | 0 | 0 | 0 | 27 |
| `.py` | 1 | 0 | 2 | 7 |
| `.rs` | 2 | 0 | 0 | 18 |
| `.sh` | 0 | 0 | 6 | 5 |
| `.sql` | 0 | 0 | 0 | 9 |
| `.toml` | 0 | 0 | 0 | 1 |
| `.yaml` | 0 | 0 | 0 | 1 |
| `.yml` | 0 | 0 | 0 | 2 |

## Detected Tooling

| Tool | Status |
| --- | --- |
| `rustfmt` | present |
| `clippy` | present |
| `markdownlint` | present |
| `editorconfig` | present |
| `pre-commit` | present |
| `gitattributes` | present |
| `contributing` | present |

## Conflict Findings

- VS Code Rust rulers [100] do not match rustfmt max_width=120.

## Recommended Stack

- Rust: `rustfmt`
- Python: `ruff format`
- Shell: `shfmt`
- Cross-file hygiene: `pre-commit-hooks` + `.editorconfig` + `.gitattributes`
- Docs style: existing `markdownlint` gate
