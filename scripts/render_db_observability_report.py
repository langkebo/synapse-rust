#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def read_text(path: Path, limit: int = 120) -> str:
    if not path.exists():
        return "_missing_"
    content = path.read_text(encoding="utf-8", errors="replace").strip()
    if not content:
        return "_empty_"
    lines = content.splitlines()
    if len(lines) > limit:
        lines = lines[:limit] + ["... (truncated)"]
    return "\n".join(lines)


def render_section(title: str, path: Path, filenames: list[str]) -> str:
    parts = [f"## {title}", ""]
    for filename in filenames:
        parts.append(f"### `{filename}`")
        parts.append("")
        parts.append("```text")
        parts.append(read_text(path / filename))
        parts.append("```")
        parts.append("")
    return "\n".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Render PostgreSQL/Redis observability snapshots into Markdown."
    )
    parser.add_argument(
        "--pg-dir",
        type=Path,
        required=True,
        help="Path to one PostgreSQL sample directory",
    )
    parser.add_argument(
        "--redis-dir",
        type=Path,
        required=True,
        help="Path to one Redis sample directory",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("artifacts/db-observability/DB_OBSERVABILITY_REPORT.md"),
        help="Output Markdown file",
    )
    args = parser.parse_args()

    args.output.parent.mkdir(parents=True, exist_ok=True)

    md = [
        "# Database Observability Report",
        "",
        f"- PostgreSQL sample: `{args.pg_dir}`",
        f"- Redis sample: `{args.redis_dir}`",
        "",
        "## Review Checklist",
        "",
        "- Confirm `pg_stat_statements` contains representative workload.",
        "- Confirm active waits and blocking locks are either empty or explained.",
        "- Confirm low-usage indexes are real cleanup candidates before drop.",
        "- Confirm Redis slowlog and latency doctor reflect production-like load.",
        "- Add owner, action, ETA, and regression risk for each hotspot.",
        "",
        render_section(
            "PostgreSQL",
            args.pg_dir,
            [
                "README.txt",
                "10_pg_stat_statements_top_total_exec.sql.txt",
                "11_pg_stat_statements_top_mean_exec.sql.txt",
                "20_pg_stat_activity_waits.sql.txt",
                "30_blocking_locks.sql.txt",
                "40_pg_stat_user_tables_hotspots.sql.txt",
                "41_pg_stat_user_indexes_low_usage.sql.txt",
                "42_pg_stat_user_tables_vacuum.sql.txt",
                "50_pg_stat_database.sql.txt",
            ],
        ),
        render_section(
            "Redis",
            args.redis_dir,
            [
                "README.txt",
                "02_info_stats.txt",
                "03_info_commandstats.txt",
                "04_info_memory.txt",
                "11_slowlog_get_128.txt",
                "12_latency_latest.txt",
                "13_latency_doctor.txt",
                "14_client_list.txt",
            ],
        ),
        "## Findings Template",
        "",
        "| Category | Object | Evidence | Risk | Recommended Action | Owner | ETA |",
        "|---|---|---|---|---|---|---|",
        "| Slow Query |  |  |  |  |  |  |",
        "| Lock Wait |  |  |  |  |  |  |",
        "| Index |  |  |  |  |  |  |",
        "| Redis Latency |  |  |  |  |  |  |",
        "",
    ]

    args.output.write_text("\n".join(md), encoding="utf-8")
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
