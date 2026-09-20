#!/usr/bin/env python3
"""Scan repository formatting drift and emit Markdown or JSON audit reports."""

from __future__ import annotations

import argparse
import json
import re
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
import tomllib
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
IGNORED_PARTS = {
    ".git",
    "target",
    "node_modules",
    ".next",
    "coverage",
    "dist",
    ".gstack",
    ".claude",
    ".scratch",
}
IGNORED_PATH_SUBSTRINGS = (
    "docker/deploy/backups/",
    "docker/artifacts/",
    "target_arm64/",
    "target_x86_64/",
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


def _git_tracked_files() -> list[Path] | None:
    """Return git-tracked files, or None if git is unavailable."""
    import shutil
    import subprocess

    if shutil.which("git") is None:
        return None
    try:
        result = subprocess.run(
            ["git", "ls-files", "-z"],
            capture_output=True,
            check=True,
        )
    except subprocess.CalledProcessError:
        return None
    paths: list[Path] = []
    for entry in result.stdout.decode("utf-8", errors="ignore").split("\x00"):
        if not entry:
            continue
        paths.append(REPO_ROOT / entry)
    return paths


def iter_files() -> list[Path]:
    files: list[Path] = []
    tracked = _git_tracked_files()
    if tracked is not None:
        # Audit only git-tracked files so build artifacts and tool-generated
        # outputs in working trees do not create noise locally vs CI parity.
        candidates = tracked
    else:
        candidates = list(REPO_ROOT.rglob("*"))
    for path in candidates:
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


_FENCE = re.compile(r"^\s*(```|~~~)")


def markdown_prose_lines(lines: list[str]) -> list[str]:
    """Return the lines of a Markdown file that are outside fenced code blocks.

    A leading tab inside a fence is *data*, not indentation — the same reasoning
    that made `tabs` count only indentation tabs (see `collect_metrics`).
    Measured 2026-09-20, the only remaining hit was
    `docs/audit/DOCKER_REVIEW_2026-09-19.md`, whose fenced ```make block holds a
    real Makefile recipe; Makefile recipes *require* a leading tab, so "fixing"
    the document would publish a broken example. Lines outside fences are still
    scanned, so a genuinely tab-indented document still counts.
    """
    prose: list[str] = []
    fence: str | None = None
    for line in lines:
        match = _FENCE.match(line)
        if match:
            marker = match.group(1)
            if fence is None:
                fence = marker
            elif fence == marker:
                fence = None
            continue
        if fence is None:
            prose.append(line)
    return prose


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
        # Only *indentation* tabs count as formatting drift. `.editorconfig`
        # declares `indent_style = space`, so a leading tab is the thing that
        # conflicts with the declared style. A tab **inside** a line is usually
        # data, and flagging it made this gate unable to go green without
        # breaking real tools: measured 2026-09-20, the only two hits were
        #   * scripts/ci/compute_perf_gate.sh — a `<bench_name>\t<ceiling_ns>`
        #     table inside a quoted heredoc, consumed by an `IFS=$'\t'` reader;
        #   * scripts/ci/benchmark_pr_gate.sh — `grep -F "${bench_name}<TAB>"`
        #     against a TSV report.
        # Replacing those tabs with spaces would silently stop both perf gates
        # from finding their baselines. The signal still catches a genuinely
        # tab-indented file (proof: a file whose first line starts with a tab is
        # still counted).
        tab_lines = markdown_prose_lines(lines) if ext == ".md" else lines
        if any("\t" in line[: len(line) - len(line.lstrip())] for line in tab_lines):
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


def build_report_data() -> dict[str, Any]:
    files = iter_files()
    counts, style = collect_metrics(files)
    return {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%SZ"),
        "repository": str(REPO_ROOT),
        "counts": dict(sorted(counts.items())),
        "style": {
            ext: {
                "trailing_ws": metrics.get("trailing_ws", 0),
                "crlf": metrics.get("crlf", 0),
                "tabs": metrics.get("tabs", 0),
                "missing_final_newline": metrics.get("missing_final_newline", 0),
            }
            for ext, metrics in sorted(style.items())
        },
        "tooling": [
            {"tool": name, "status": status} for name, status in detect_tooling()
        ],
        "conflicts": detect_conflicts(),
        "recommended_stack": [
            "Rust: `rustfmt`",
            "Python: `ruff format`",
            "Shell: `shfmt`",
            "Cross-file hygiene: `pre-commit-hooks` + `.editorconfig` + `.gitattributes`",
            "Docs style: existing `markdownlint` gate",
        ],
    }


def render_markdown(data: dict[str, Any]) -> str:
    generated_at = data["generated_at"]
    counts = data["counts"]
    style = data["style"]
    lines = [
        "# Format Drift Audit",
        "",
        f"- Generated at: `{generated_at}`",
        f"- Repository: `{data['repository']}`",
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
    for tooling in data["tooling"]:
        lines.append(f"| `{tooling['tool']}` | {tooling['status']} |")

    conflicts = data["conflicts"]
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
            *[f"- {item}" for item in data["recommended_stack"]],
        ]
    )
    return "\n".join(lines) + "\n"


def render_json(data: dict[str, Any]) -> str:
    return json.dumps(data, indent=2, sort_keys=True) + "\n"


def has_drift(style: dict[str, Counter] | dict[str, dict[str, int]]) -> bool:
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
        "--json-output", type=Path, help="Write the JSON report to this path."
    )
    parser.add_argument(
        "--fail-on-drift",
        action="store_true",
        help="Exit with status 1 when formatting drift is detected.",
    )
    args = parser.parse_args()

    data = build_report_data()
    report = render_markdown(data)

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(report, encoding="utf-8")
    else:
        print(report)

    if args.json_output:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(render_json(data), encoding="utf-8")

    if args.fail_on_drift and has_drift(data["style"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
