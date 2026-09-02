#!/usr/bin/env python3
"""
refresh_openapi_specs.py — 一键刷新所有 profile 的 OpenAPI spec

执行流:
  1. 调 export_ledger.sh 为每个 profile (default/oidc/worker/saml/all) 导出 ledger JSON
  2. 调 generate_openapi.py --all-profiles 生成 client-{profile}.yaml + index.json
  3. 输出 changelog: 哪些端点新增/删除/路径变化

依赖: export_ledger.sh (在同目录) + generate_openapi.py (在同目录)
      cargo (~3 min 首次编译, 后续增量秒级)
      synapse_ledger_export binary

用法:
  # 完整刷新 (需要 cargo)
  python3 scripts/api_test/refresh_openapi_specs.py

  # 只对 default profile (快, 无 cargo 编译)
  python3 scripts/api_test/refresh_openapi_specs.py --profile default

  # 自定义 profile 列表
  python3 scripts/api_test/refresh_openapi_specs.py --profiles default,oidc

  # 跳过 export (假定 ledger 已就绪)
  python3 scripts/api_test/refresh_openapi_specs.py --skip-export
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent

ALL_PROFILES = ["default", "oidc", "worker", "saml", "all"]

# 路径配置
EXPORT_SCRIPT = SCRIPT_DIR / "export_ledger.sh"
REPORTS_DIR = SCRIPT_DIR / "reports"
OPENAPI_DIR = PROJECT_ROOT / "docs" / "openapi"
LEDGER_DEFAULT = SCRIPT_DIR / "ledger.json"  # 兼容: export_ledger.sh 默认输出到 scripts/api_test/ledger.json

GENERATOR_SCRIPT = SCRIPT_DIR / "generate_openapi.py"


def export_ledger(profile: str) -> Path:
    """调 export_ledger.sh 导出指定 profile 的 ledger. 返回 ledger 路径."""
    output_path = REPORTS_DIR / f"ledger_{profile}.json"
    output_path.parent.mkdir(parents=True, exist_ok=True)

    if not EXPORT_SCRIPT.exists():
        raise FileNotFoundError(f"export script not found: {EXPORT_SCRIPT}")

    print(f"  [export] profile={profile} → {output_path}")
    cmd = ["bash", str(EXPORT_SCRIPT), f"--profile={profile}", f"--output={output_path}"]
    result = subprocess.run(cmd, capture_output=True, text=True, cwd=PROJECT_ROOT)
    if result.returncode != 0:
        print(f"  [export] FAILED: {result.stderr[:500]}", file=sys.stderr)
        raise RuntimeError(f"export_ledger.sh failed for profile={profile}")
    print(f"  [export] OK: {output_path.name}")
    return output_path


def run_generator(profiles: list[str]) -> None:
    """调 generate_openapi.py --all-profiles 生成所有 spec + index.json."""
    cmd = [sys.executable, str(GENERATOR_SCRIPT), "--all-profiles", "--profiles", ",".join(profiles)]
    result = subprocess.run(cmd, capture_output=True, text=True, cwd=PROJECT_ROOT)
    print(result.stdout, end="")
    if result.returncode != 0:
        print(f"  [generate] FAILED: {result.stderr[:500]}", file=sys.stderr)
        raise RuntimeError("generate_openapi.py --all-profiles failed")


def compute_changelog(old_index: dict | None, new_index: dict) -> dict:
    """对比新旧 index.json, 计算端点变化. 简化版 — 只比较 client_server_endpoints 数量."""
    return {
        "old_endpoint_count": sum(s.get("client_server_endpoints", 0) for s in (old_index or {}).get("specs", {}).values()),
        "new_endpoint_count": sum(s.get("client_server_endpoints", 0) for s in new_index.get("specs", {}).values()),
        "old_index": old_index,
        "new_index": new_index,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--profile", default="", help="单 profile 模式 (例: default)")
    ap.add_argument("--profiles", default="", help="自定义 profile 列表 (例: default,oidc)")
    ap.add_argument("--skip-export", action="store_true", help="跳过 export_ledger.sh,假定 ledger 已就绪")
    ap.add_argument("--skip-generate", action="store_true", help="跳过 generate_openapi.py,只做 export")
    ap.add_argument("--index", default=str(OPENAPI_DIR / "index.json"), help="Manifest index.json 路径")
    args = ap.parse_args()

    # 决定 profile 列表
    if args.profile:
        profiles = [args.profile]
    elif args.profiles:
        profiles = [p.strip() for p in args.profiles.split(",") if p.strip()]
    else:
        profiles = list(ALL_PROFILES)

    print(f"[refresh] target profiles: {profiles}")
    print(f"[refresh] project root: {PROJECT_ROOT}")
    print()

    # Phase 1: Export
    if not args.skip_export:
        print("=" * 60)
        print("Phase 1: Export ledgers (cargo run --bin synapse_ledger_export)")
        print("=" * 60)
        for profile in profiles:
            try:
                export_ledger(profile)
            except (FileNotFoundError, RuntimeError) as e:
                print(f"  [export] SKIP {profile}: {e}")
        print()
    else:
        print("[refresh] --skip-export: using existing ledger files")
        print()

    # Phase 2: Generate OpenAPI
    if not args.skip_generate:
        print("=" * 60)
        print("Phase 2: Generate OpenAPI specs")
        print("=" * 60)
        run_generator(profiles)
        print()

    # Phase 3: Changelog
    index_path = Path(args.index)
    if index_path.exists():
        new_index = json.loads(index_path.read_text(encoding="utf-8"))
        # 找上一个 index (git 历史或旧文件)
        old_index = None
        old_index_path = index_path.with_suffix(".old.json")
        if old_index_path.exists():
            old_index = json.loads(old_index_path.read_text(encoding="utf-8"))

        changelog = compute_changelog(old_index, new_index)
        old_count = changelog["old_endpoint_count"]
        new_count = changelog["new_endpoint_count"]
        delta = new_count - old_count
        print("=" * 60)
        print("Changelog")
        print("=" * 60)
        print(f"  Previous: {old_count} endpoints")
        print(f"  Current:  {new_count} endpoints")
        if delta > 0:
            print(f"  +{delta} new endpoint(s)")
        elif delta < 0:
            print(f"  {delta} removed endpoint(s)")
        else:
            print("  No count change (但 path/method 可能有变化,需 diff)")

        # 列每个 profile 的统计
        for name, spec in new_index.get("specs", {}).items():
            print(f"  [{name:10s}] {spec['client_server_endpoints']:4d} endpoints, {spec['operations']:4d} ops, {spec['tags']:2d} tags")

    print()
    print("[refresh] done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
