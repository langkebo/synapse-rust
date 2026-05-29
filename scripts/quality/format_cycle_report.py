#!/usr/bin/env python3
"""Generate a rolling three-cycle format drift tracking report."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any

from format_audit import build_report_data, has_drift


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = REPO_ROOT / "docs" / "quality" / "FORMAT_DRIFT_TRACKING.md"
STATE_START = "<!-- format-drift-tracking-state:start -->"
STATE_END = "<!-- format-drift-tracking-state:end -->"
MAX_CYCLES = 3
ALLOWED_ROOT_CONFIGS = {
    ".editorconfig",
    ".gitattributes",
    ".pre-commit-config.yaml",
    "rustfmt.toml",
    ".markdownlint.json",
}


def git_output(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def is_nested_format_config(path_str: str) -> bool:
    path = Path(path_str)
    if len(path.parts) <= 1:
        return False

    name = path.name
    if path_str in ALLOWED_ROOT_CONFIGS:
        return False

    formatter_prefixes = (".prettierrc", ".eslintrc")
    formatter_names = {
        ".editorconfig",
        ".gitattributes",
        ".pre-commit-config.yaml",
        ".prettierignore",
        ".clang-format",
        ".clang-format-ignore",
        "rustfmt.toml",
    }
    if name in formatter_names:
        return True
    return name.startswith(formatter_prefixes)


def load_existing_history(report_path: Path) -> list[dict[str, Any]]:
    if not report_path.exists():
        return []

    text = report_path.read_text(encoding="utf-8")
    start = text.find(STATE_START)
    end = text.find(STATE_END)
    if start == -1 or end == -1 or end <= start:
        return []

    payload = text[start + len(STATE_START) : end].strip()
    if not payload:
        return []

    try:
        state = json.loads(payload)
    except json.JSONDecodeError:
        return []
    return state.get("history", [])


def tracked_nested_configs() -> list[str]:
    files = git_output("ls-files").splitlines()
    return sorted(path for path in files if is_nested_format_config(path))


def changed_files(base_ref: str, head_ref: str) -> list[str]:
    output = git_output("diff", "--name-only", f"{base_ref}..{head_ref}")
    if not output:
        return []
    return [line for line in output.splitlines() if line]


def count_tracked_format_files(paths: list[str]) -> int:
    tracked_suffixes = {
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
    return sum(1 for path in paths if Path(path).suffix.lower() in tracked_suffixes)


def summarize_drift(style: dict[str, dict[str, int]]) -> dict[str, int]:
    totals = {
        "trailing_ws": 0,
        "crlf": 0,
        "tabs": 0,
        "missing_final_newline": 0,
    }
    for metrics in style.values():
        for key in totals:
            totals[key] += metrics.get(key, 0)
    return totals


def build_cycle_entry(
    cycle_label: str,
    base_ref: str,
    head_ref: str,
    compliance_status: str,
) -> dict[str, Any]:
    audit = build_report_data()
    all_nested_configs = tracked_nested_configs()
    range_files = changed_files(base_ref, head_ref)
    new_nested_configs = sorted(
        path for path in range_files if is_nested_format_config(path)
    )
    head_commit = git_output("rev-parse", "--short", head_ref)
    commit_count_raw = git_output("rev-list", "--count", f"{base_ref}..{head_ref}")
    drift_totals = summarize_drift(audit["style"])
    drift_signal_total = sum(drift_totals.values())

    entry = {
        "cycle_label": cycle_label,
        "generated_at": audit["generated_at"],
        "base_ref": base_ref,
        "head_ref": head_ref,
        "head_commit": head_commit,
        "commit_count": int(commit_count_raw) if commit_count_raw else 0,
        "changed_file_count": len(range_files),
        "changed_format_file_count": count_tracked_format_files(range_files),
        "compliance_status": compliance_status,
        "drift_signal_total": drift_signal_total,
        "drift_totals": drift_totals,
        "nested_config_count": len(all_nested_configs),
        "nested_configs": all_nested_configs,
        "new_nested_configs": new_nested_configs,
        "conflicts": audit["conflicts"],
        "status": "pass"
        if compliance_status == "pass"
        and drift_signal_total == 0
        and not new_nested_configs
        else "needs_attention",
    }
    return entry


def cycle_status_badge(status: str) -> str:
    return "PASS" if status == "pass" else "ATTN"


def render_report(history: list[dict[str, Any]]) -> str:
    latest = history[0]
    lines = [
        "# Format Drift Tracking",
        "",
        "This report keeps the most recent three delivery-cycle checks after the repository-wide formatting rollout.",
        "Update it after each release train, sprint handoff, or other agreed delivery checkpoint.",
        "",
        "## Workflow",
        "",
        "1. Run `make format-check` or wait for the scheduled `Format Drift Tracking` workflow.",
        "2. Run `make format-cycle CYCLE_LABEL=<cycle-name>` to refresh this report locally.",
        "3. Review any new conflicts, nested formatter configs, or drift signals before the next cycle starts.",
        "",
        "## Latest Snapshot",
        "",
        f"- Latest cycle: `{latest['cycle_label']}`",
        f"- Range: `{latest['base_ref']}..{latest['head_ref']}`",
        f"- Head commit: `{latest['head_commit']}`",
        f"- Compliance status: `{latest['compliance_status']}`",
        f"- Drift signals: `{latest['drift_signal_total']}`",
        f"- Newly introduced nested formatter configs: `{len(latest['new_nested_configs'])}`",
        "",
        "## Cycle Log",
        "",
        "| Cycle | Range | Commits | Changed Format Files | Drift Signals | New Nested Configs | Compliance | Status |",
        "| --- | --- | ---: | ---: | ---: | ---: | --- | --- |",
    ]

    for entry in history:
        lines.append(
            "| `{}` | `{}` | {} | {} | {} | {} | `{}` | `{}` |".format(
                entry["cycle_label"],
                f"{entry['base_ref']}..{entry['head_ref']}",
                entry["commit_count"],
                entry["changed_format_file_count"],
                entry["drift_signal_total"],
                len(entry["new_nested_configs"]),
                entry["compliance_status"],
                cycle_status_badge(entry["status"]),
            )
        )

    lines.extend(
        [
            "",
            "## Current Gates",
            "",
            "- `PASS` means the cycle check ran after a successful compliance run, found zero drift signals, and did not introduce new nested formatter configs.",
            "- `ATTN` means at least one of those conditions failed and maintainers should inspect the cycle details before closing the checkpoint.",
            "",
            "## Latest Cycle Details",
            "",
            f"- Cycle label: `{latest['cycle_label']}`",
            f"- Generated at: `{latest['generated_at']}`",
            f"- Changed files in range: `{latest['changed_file_count']}`",
            f"- Changed format-scoped files in range: `{latest['changed_format_file_count']}`",
            "- Drift totals: "
            f"`trailing_ws={latest['drift_totals']['trailing_ws']}`, "
            f"`crlf={latest['drift_totals']['crlf']}`, "
            f"`tabs={latest['drift_totals']['tabs']}`, "
            f"`missing_final_newline={latest['drift_totals']['missing_final_newline']}`",
            f"- Tracked nested formatter configs: `{latest['nested_config_count']}`",
        ]
    )

    if latest["new_nested_configs"]:
        lines.extend(["", "## New Nested Configs In Latest Cycle", ""])
        for path in latest["new_nested_configs"]:
            lines.append(f"- `{path}`")

    if latest["nested_configs"]:
        lines.extend(["", "## Tracked Nested Formatter Configs", ""])
        for path in latest["nested_configs"]:
            lines.append(f"- `{path}`")

    if latest["conflicts"]:
        lines.extend(["", "## Open Conflicts", ""])
        for conflict in latest["conflicts"]:
            lines.append(f"- {conflict}")
    else:
        lines.extend(
            ["", "## Open Conflicts", "", "- None detected in the latest audit."]
        )

    state = json.dumps({"history": history}, indent=2, ensure_ascii=True)
    lines.extend(["", STATE_START, state, STATE_END, ""])
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--cycle-label", required=True, help="Human-readable cycle name."
    )
    parser.add_argument(
        "--base-ref", default="HEAD~1", help="Git base ref for the cycle range."
    )
    parser.add_argument(
        "--head-ref", default="HEAD", help="Git head ref for the cycle range."
    )
    parser.add_argument(
        "--compliance-status",
        choices=("pass", "unknown", "fail"),
        default="unknown",
        help="Compliance result associated with this cycle snapshot.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="Tracking report destination.",
    )
    args = parser.parse_args()

    entry = build_cycle_entry(
        cycle_label=args.cycle_label,
        base_ref=args.base_ref,
        head_ref=args.head_ref,
        compliance_status=args.compliance_status,
    )

    history = [entry]
    for existing in load_existing_history(args.output):
        if existing.get("cycle_label") == entry["cycle_label"]:
            continue
        history.append(existing)
    history = history[:MAX_CYCLES]

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(render_report(history), encoding="utf-8")

    if entry["status"] != "pass":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
