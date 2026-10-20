#!/usr/bin/env python3
"""
gen_route_table.py — 生成 docs/openapi/route-table.json（CI artifact）

从 ledger JSON 读取所有端点，输出扁平 JSON 路由表。

入库的两个视图（D-15，坐标定义见 docs/openapi/README.md）:
  * `docs/openapi/route-table.json`     — golden lane × default profile
    （CI 用 fresh default-feature 导出生成并 --check 对账）
  * `docs/openapi/route-table-sdk.json` — sdk lane × all profile（最大完整面）
    源 = `tests/unit/fixtures/ledger_export_sdk/all.json`，由
    `scripts/contract/check_route_surface_reconciliation.py` 的 R6 对账

只给 golden 视图会让消费者把整族 feature-gated 路由（`/voice/*` 等）判成未解析 —— 因此
SDK 消费方必须用 sdk 视图。

用法:
  python3 scripts/api_test/gen_route_table.py --ledger PATH [--output PATH]   # --ledger 必填
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
        print(
            f"[gen_route_table] ERROR: ledger not found: {ledger_path}", file=sys.stderr
        )
        sys.exit(1)
    data = json.loads(ledger_path.read_text(encoding="utf-8"))
    if "entries" not in data:
        print("[gen_route_table] ERROR: ledger missing 'entries' key", file=sys.stderr)
        sys.exit(1)
    return data


def derive_lane(profile_flags: dict | None) -> str | None:
    """Feature lane of the source ledger, derived from `profile_flags`.

    The two lanes (`golden` = default features, `sdk` = default + all-extensions)
    differ by exactly one compile-time feature, `saml-sso`, which surfaces as
    `profile_flags.saml_enabled` (see docs/openapi/README.md「权威口径与
    (lane, profile) 坐标轴」). A new lane-dividing feature would invalidate this
    derivation and must be revisited here.
    """
    if profile_flags is None:
        return None
    return "sdk" if profile_flags.get("saml_enabled") else "golden"


def build_route_table(ledger: dict) -> dict:
    """Build a flat route table from ledger entries.

    The artifact is self-describing about the coordinate it was cut from (BE-1):
    alongside the ledger's own `schema_version`, it carries the source ledger's
    `schema_version` (`ledger_schema_version`), the derived feature `lane`, and
    the source `profile_flags`. `schema_version` here is *this artifact's* output
    shape version and is unrelated to the ledger's `schema_version` — the two
    names are deliberately kept distinct (BE-1 renamed the ambiguity away by
    adding `ledger_schema_version`).
    """
    entries = sorted(
        ledger["entries"],
        key=lambda e: (e["path"], e["method"], e["registered_by"]),
    )
    profile_flags = ledger.get("profile_flags")
    return {
        "schema_version": "1",
        "ledger_schema_version": ledger.get("schema_version"),
        "lane": derive_lane(profile_flags),
        "generated_at": ledger.get("generated_at", ""),
        "source": "synapse_ledger_export",
        "profile": ledger.get("state_profile", "unknown"),
        "profile_flags": profile_flags,
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
    ap.add_argument(
        "--ledger",
        required=True,
        help=(
            "Path to ledger JSON（**必填**）。"
            "此前默认指向 `scripts/api_test/ledger.json` —— 那是 2026-08-12 的陈旧导出，"
            "照默认跑会产出 3000+ 行伪改动（路由集不同），且本地 `--check` 与陈旧源自洽、"
            "只在 CI 判红（已踩 2 次：`a421e7641`、2026-10-09，见后端 UNRESOLVED_ISSUES_SUMMARY DOC-13）。"
            "CI 口径：用 `synapse_ledger_export --profile=default --timestamp=<artifact_common.FIXED_TIMESTAMP>` "
            "现导出的 JSON 传进来。"
        ),
    )
    ap.add_argument(
        "--output", default=str(OUTPUT_FILE), help="Output path for route-table.json"
    )
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
            print(
                f"[gen_route_table] OK: {expected_path} matches a fresh generation from {args.ledger}"
            )
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
    print(
        f"[gen_route_table] generated {output_path} ({route_table['total_routes']} routes)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
