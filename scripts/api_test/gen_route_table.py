#!/usr/bin/env python3
"""
gen_route_table.py — 生成 docs/openapi/route-table.json（CI artifact）

从 ledger JSON 读取所有端点，输出扁平 JSON 路由表。

用法:
  python3 scripts/api_test/gen_route_table.py [--ledger PATH] [--output PATH]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from artifact_common import describe_drift

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent

DEFAULT_LEDGER = SCRIPT_DIR / "ledger.json"
OUTPUT_DIR = PROJECT_ROOT / "docs" / "openapi"
OUTPUT_FILE = OUTPUT_DIR / "route-table.json"


def load_ledger(ledger_path: Path) -> dict:
    """Load and validate ledger JSON."""
    if not ledger_path.exists():
        print(f"[gen_route_table] ERROR: ledger not found: {ledger_path}", file=sys.stderr)
        sys.exit(1)
    data = json.loads(ledger_path.read_text(encoding="utf-8"))
    if "entries" not in data:
        print("[gen_route_table] ERROR: ledger missing 'entries' key", file=sys.stderr)
        sys.exit(1)
    return data


def build_route_table(ledger: dict) -> dict:
    """Build a flat route table from ledger entries."""
    entries = sorted(
        ledger["entries"],
        key=lambda e: (e["path"], e["method"], e["registered_by"]),
    )
    return {
        "schema_version": "1",
        "generated_at": ledger.get("generated_at", ""),
        "source": "synapse_ledger_export",
        "profile": ledger.get("state_profile", "unknown"),
        "total_routes": len(entries),
        "_meta": {
            "generated_by": "gen_route_table.py",
            "note": "本文件由 CI 自动生成，禁止手改。如需刷新: python3 scripts/api_test/gen_route_table.py",
        },
        "routes": [
            {
                "method": e["method"],
                "path": e["path"],
                "registered_by": e["registered_by"],
                "path_params": e.get("path_params", []),
                "query_params": e.get("query_params", []),
                "auth": e.get("auth"),
            }
            for e in entries
        ],
    }


def render_table(route_table: dict) -> str:
    """Canonical serialization — the single byte-for-byte definition of the artifact."""
    return json.dumps(route_table, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate route-table.json from ledger")
    ap.add_argument("--ledger", default=str(DEFAULT_LEDGER), help="Path to ledger JSON")
    ap.add_argument("--output", default=str(OUTPUT_FILE), help="Output path for route-table.json")
    ap.add_argument(
        "--check",
        action="store_true",
        help="Do not write: regenerate from --ledger and diff against --expected; exit 1 on drift",
    )
    ap.add_argument(
        "--expected",
        default=str(OUTPUT_FILE),
        help="Reference artifact for --check (default: committed docs/openapi/route-table.json)",
    )
    args = ap.parse_args()

    ledger = load_ledger(Path(args.ledger))
    route_table = build_route_table(ledger)
    rendered = render_table(route_table)

    if args.check:
        expected_path = Path(args.expected)
        if not expected_path.exists():
            print(
                f"[gen_route_table] CHECK FAILED: reference artifact not found: {expected_path}",
                file=sys.stderr,
            )
            return 1
        expected = expected_path.read_text(encoding="utf-8")
        if expected == rendered:
            print(f"[gen_route_table] OK: {expected_path} matches a fresh generation from {args.ledger}")
            return 0
        print(
            f"[gen_route_table] CHECK FAILED: {expected_path} is stale — it differs from a fresh "
            f"generation from {args.ledger} ({route_table['total_routes']} routes).",
            file=sys.stderr,
        )
        print(describe_drift(expected, rendered), file=sys.stderr)
        print(
            "    Regenerate and commit it with:\n"
            f"      python3 scripts/api_test/gen_route_table.py --ledger {args.ledger}",
            file=sys.stderr,
        )
        return 1

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(rendered, encoding="utf-8")
    print(f"[gen_route_table] generated {output_path} ({route_table['total_routes']} routes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
