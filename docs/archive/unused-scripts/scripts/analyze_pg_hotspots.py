#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
from dataclasses import dataclass
from pathlib import Path


@dataclass
class QueryEntry:
    source: str
    queryid: str
    calls: str
    mean_exec_time: str
    total_exec_time: str
    rows: str
    query: str


def read_tsv(path: Path) -> list[dict[str, str]]:
    if not path.exists():
        return []
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    rows = [line for line in lines if line and not line.startswith("--")]
    if len(rows) < 2:
        return []
    reader = csv.DictReader(rows, delimiter="\t")
    return list(reader)


def classify_query(sql: str) -> tuple[str, str, str]:
    normalized = " ".join(sql.lower().split())
    if "offset" in normalized and "limit" in normalized:
        return (
            "Deep Pagination",
            "High",
            "Replace OFFSET pagination with keyset pagination on stable ordered keys.",
        )
    if "lower(" in normalized and " like " in normalized:
        return (
            "Function Search",
            "High",
            "Prefer trigram/FTS or dedicated function indexes with tighter predicates.",
        )
    if " ilike " in normalized:
        return (
            "ILIKE Search",
            "High",
            "Consider trigram index, FTS, or search side table to avoid wide scans.",
        )
    if "content::text like" in normalized or "lower(content::text)" in normalized:
        return (
            "JSON Text Search",
            "High",
            "Move content search to FTS or a dedicated search projection table.",
        )
    if "count(distinct" in normalized:
        return (
            "Distinct Aggregation",
            "Medium",
            "Pre-aggregate counts or maintain summary tables/materialized projections.",
        )
    if "stream_ordering" in normalized:
        return (
            "Event Stream",
            "High",
            "Validate covering indexes and keep stream_ordering backfill/indexes healthy.",
        )
    if " on conflict " in normalized:
        return (
            "Upsert Path",
            "Medium",
            "Check conflict target selectivity and remove unnecessary updates on no-op writes.",
        )
    if "join " in normalized and normalized.count(" join ") >= 3:
        return (
            "Multi-Join Query",
            "Medium",
            "Validate join order, selective predicates, and covering indexes on join keys.",
        )
    return (
        "General Query",
        "Medium",
        "Review with EXPLAIN (ANALYZE, BUFFERS) and tune indexes or SQL shape.",
    )


def build_entries(sample_dir: Path) -> list[QueryEntry]:
    files = [
        ("total_exec", sample_dir / "10_pg_stat_statements_top_total_exec.sql.txt"),
        ("mean_exec", sample_dir / "11_pg_stat_statements_top_mean_exec.sql.txt"),
    ]
    entries: list[QueryEntry] = []
    for source, path in files:
        for row in read_tsv(path):
            entries.append(
                QueryEntry(
                    source=source,
                    queryid=row.get("queryid", ""),
                    calls=row.get("calls", ""),
                    mean_exec_time=row.get(
                        "mean_exec_time", row.get("mean_exec_time ", "")
                    ),
                    total_exec_time=row.get("total_exec_time", ""),
                    rows=row.get("rows", ""),
                    query=row.get("query", "").strip(),
                )
            )
    return entries


def unique_entries(entries: list[QueryEntry]) -> list[QueryEntry]:
    seen: set[tuple[str, str]] = set()
    deduped: list[QueryEntry] = []
    for entry in entries:
        key = (entry.queryid, entry.query)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(entry)
    return deduped


def render_markdown(entries: list[QueryEntry], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# PostgreSQL Hotspot Analysis",
        "",
        "| Rank | Category | Severity | queryid | Source | Calls | Mean ms | Total ms | Recommended Action |",
        "|---:|---|---|---|---|---:|---:|---:|---|",
    ]
    for idx, entry in enumerate(entries[:20], start=1):
        category, severity, action = classify_query(entry.query)
        lines.append(
            f"| {idx} | {category} | {severity} | `{entry.queryid}` | `{entry.source}` | "
            f"{entry.calls or '0'} | {entry.mean_exec_time or '0'} | {entry.total_exec_time or '0'} | {action} |"
        )
    lines.extend(["", "## SQL Samples", ""])
    for idx, entry in enumerate(entries[:20], start=1):
        category, severity, action = classify_query(entry.query)
        lines.extend(
            [
                f"### {idx}. {category} / {severity}",
                "",
                f"- `queryid`: `{entry.queryid}`",
                f"- `source`: `{entry.source}`",
                f"- `calls`: `{entry.calls or '0'}`",
                f"- `mean_exec_time_ms`: `{entry.mean_exec_time or '0'}`",
                f"- `total_exec_time_ms`: `{entry.total_exec_time or '0'}`",
                f"- `recommended_action`: {action}",
                "",
                "```sql",
                entry.query or "-- empty query text",
                "```",
                "",
            ]
        )
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Classify PostgreSQL pg_stat_statements hotspots."
    )
    parser.add_argument(
        "--sample-dir",
        type=Path,
        required=True,
        help="Path to one PostgreSQL sample directory",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("artifacts/db-observability/PG_HOTSPOT_ANALYSIS.md"),
        help="Markdown output path",
    )
    args = parser.parse_args()

    entries = unique_entries(build_entries(args.sample_dir))
    render_markdown(entries, args.output)
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
