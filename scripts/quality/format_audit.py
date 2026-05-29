#!/usr/bin/env python3
"""Scan repository formatting drift and emit a Markdown audit report."""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
import tomllib


REPO_ROOT = Path(__file__).resolve().parents[2]
IGNORED_PARTS = {".git", "target", "node_modules", ".next", "coverage", "dist"}
IGNORED_PATH_SUBSTRINGS = (
    "docker/deploy/backups/",
    "docker/artifacts/",
    "artifacts/sqlx-migrations/",
    "artifacts/sqlx-migrations-test/",
)
SCANNED_EXTENSIONS = {
    ".rs",
    ".md",
    ".sql",
    ".sh",
    ".json",
    ".yml",
    ".yaml",
    ".py",
    ".toml",
}


def iter_files() -> list[Path]:
    files: list[Path] = []
    for path in REPO_ROOT.rglob("*"):
        if not path.is_file():
            continue
        if any(part in IGNORED_PARTS for part in path.parts):
            continue
        relative_path = path.relative_to(REPO_ROOT).as_posix()
        if any(marker in relative_path for marker in IGNORED_PATH_SUBSTRINGS):
            continue
        files.append(path)
    return files


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\x00" in data:
        return None
    return data.decode("utf-8", errors="ignore")


def collect_metrics(files: list[Path]) -> tuple[Counter, dict[str, Counter]]:
    counts: Counter[str] = Counter()
    style: dict[str, Counter] = defaultdict(Counter)
    for path in files:
        ext = path.suffix.lower() or "<no_ext>"
        if ext not in SCANNED_EXTENSIONS:
            continue
        counts[ext] += 1
        text = read_text(path)
        if text is None:
            continue
        lines = text.splitlines()
        if ext != ".md" and any(line.rstrip(" \t") != line for line in lines):
            style[ext]["trailing_ws"] += 1
        if "\r\n" in text:
            style[ext]["crlf"] += 1
        if any("\t" in line for line in lines):
            style[ext]["tabs"] += 1
        if text and not text.endswith("\n"):
            style[ext]["missing_final_newline"] += 1
    return counts, style


def detect_tooling() -> list[tuple[str, str]]:
    checks = [
        ("rustfmt", "rustfmt.toml"),
        ("clippy", ".clippy.toml"),
        ("markdownlint", ".markdownlint.json"),
        ("editorconfig", ".editorconfig"),
        ("pre-commit", ".pre-commit-config.yaml"),
        ("gitattributes", ".gitattributes"),
        ("contributing", "CONTRIBUTING.md"),
    ]
    results: list[tuple[str, str]] = []
    for name, relative in checks:
        results.append(
            (name, "present" if (REPO_ROOT / relative).exists() else "missing")
        )
    return results


def detect_conflicts() -> list[str]:
    conflicts: list[str] = []

    rustfmt_path = REPO_ROOT / "rustfmt.toml"
    vscode_path = REPO_ROOT / ".vscode" / "settings.json"
    if rustfmt_path.exists() and vscode_path.exists():
        rustfmt_config = tomllib.loads(rustfmt_path.read_text(encoding="utf-8"))
        vscode_config = json.loads(vscode_path.read_text(encoding="utf-8"))
        rust_width = rustfmt_config.get("max_width")
        rust_rulers = vscode_config.get("[rust]", {}).get("editor.rulers", [])
        if rust_width and rust_rulers and rust_width not in rust_rulers:
            conflicts.append(
                f"VS Code Rust rulers {rust_rulers} do not match rustfmt max_width={rust_width}."
            )

    if not (REPO_ROOT / ".editorconfig").exists():
        conflicts.append(
            "Root .editorconfig is missing, so editors can drift on EOL/indentation."
        )

    if not (REPO_ROOT / ".pre-commit-config.yaml").exists():
        conflicts.append(
            "No pre-commit hook configuration is present to block formatting regressions."
        )

    return conflicts


def render_markdown(counts: Counter, style: dict[str, Counter]) -> str:
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%SZ")
    lines = [
        "# Format Drift Audit",
        "",
        f"- Generated at: `{generated_at}`",
        f"- Repository: `{REPO_ROOT}`",
        "",
        "## File Distribution",
        "",
        "| Extension | Files |",
        "| --- | ---: |",
    ]
    for ext, count in sorted(counts.items(), key=lambda item: (-item[1], item[0])):
        lines.append(f"| `{ext}` | {count} |")

    lines.extend(
        [
            "",
            "## Formatting Drift Signals",
            "",
            "| Extension | Trailing WS | CRLF | Tabs | Missing Final Newline |",
            "| --- | ---: | ---: | ---: | ---: |",
        ]
    )
    for ext in sorted(counts):
        metrics = style.get(ext, Counter())
        lines.append(
            "| `{}` | {} | {} | {} | {} |".format(
                ext,
                metrics.get("trailing_ws", 0),
                metrics.get("crlf", 0),
                metrics.get("tabs", 0),
                metrics.get("missing_final_newline", 0),
            )
        )

    lines.extend(
        [
            "",
            "## Detected Tooling",
            "",
            "| Tool | Status |",
            "| --- | --- |",
        ]
    )
    for name, status in detect_tooling():
        lines.append(f"| `{name}` | {status} |")

    conflicts = detect_conflicts()
    lines.extend(["", "## Conflict Findings", ""])
    if conflicts:
        for conflict in conflicts:
            lines.append(f"- {conflict}")
    else:
        lines.append("- No direct configuration conflict detected.")

    lines.extend(
        [
            "",
            "## Recommended Stack",
            "",
            "- Rust: `rustfmt`",
            "- Python: `ruff format`",
            "- Shell: `shfmt`",
            "- Cross-file hygiene: `pre-commit-hooks` + `.editorconfig` + `.gitattributes`",
            "- Docs style: existing `markdownlint` gate",
        ]
    )
    return "\n".join(lines) + "\n"


def has_drift(style: dict[str, Counter]) -> bool:
    drift_keys = ("trailing_ws", "crlf", "tabs", "missing_final_newline")
    return any(
        metrics.get(key, 0) > 0 for metrics in style.values() for key in drift_keys
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output", type=Path, help="Write the Markdown report to this path."
    )
    parser.add_argument(
        "--fail-on-drift",
        action="store_true",
        help="Exit with status 1 when formatting drift is detected.",
    )
    args = parser.parse_args()

    files = iter_files()
    counts, style = collect_metrics(files)
    report = render_markdown(counts, style)

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(report, encoding="utf-8")
    else:
        print(report)

    if args.fail_on_drift and has_drift(style):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
