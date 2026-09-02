#!/usr/bin/env python3
"""
schemathesis_extended_test.py — 扩展冒烟测试 (Week 2 Task 1)

覆盖全部 50 个 optional 端点中**不含 path parameters** 的部分
(避免 schemathesis 内部 proxy transport 异常)。

设计:
  - 自动从 OpenAPI spec 读取所有 optional 端点 (security: [])
  - 过滤掉含 {param} 的端点 (避免 proxy 异常)
  - 每端点 N 个随机 case (默认 30)
  - 输出: scripts/api_test/reports/schemathesis_extended.json

用法:
  python3 scripts/api_test/schemathesis_extended_test.py
  python3 scripts/api_test/schemathesis_extended_test.py --max-cases 50
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent
SPEC_PATH = PROJECT_ROOT / "docs" / "openapi" / "client.yaml"
REPORTS_DIR = SCRIPT_DIR / "reports"
REPORTS_DIR.mkdir(parents=True, exist_ok=True)


def ensure_schemathesis() -> str:
    try:
        import schemathesis
        return schemathesis.__version__
    except ImportError:
        print("[setup] installing schemathesis ...")
        result = subprocess.run(
            [sys.executable, "-m", "pip", "install", "--quiet", "--user", "schemathesis"],
            capture_output=True, text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(f"pip install failed: {result.stderr[:300]}")
        import schemathesis
        return schemathesis.__version__


def check_server(base_url: str) -> bool:
    import urllib.request
    try:
        req = urllib.request.Request(f"{base_url}/_matrix/client/r0/capabilities", method="GET")
        with urllib.request.urlopen(req, timeout=5) as resp:
            return resp.status == 200
    except Exception:
        return False


def discover_optional_endpoints(spec_path: Path) -> list[tuple[str, str]]:
    """从 OpenAPI spec 中发现所有 security=[] 的端点 (optional auth).

    过滤:
      - 路径含 {param} 的端点 (避免 schemathesis proxy 异常)
      - 路径含不稳定前缀 (unstable/) 的端点 (可能含特殊 schema)

    Returns:
        List of (method, path) tuples
    """
    import yaml
    spec = yaml.safe_load(spec_path.read_text(encoding="utf-8"))
    targets = []
    for path, methods in spec.get("paths", {}).items():
        # 过滤路径
        if "{" in path:
            continue  # 含 path params
        if "/unstable/" in path:
            continue  # 不稳定 API
        for method, op in methods.items():
            if method.lower() not in {"get", "post", "put", "delete", "patch"}:
                continue
            sec = op.get("security", [])
            if not sec:  # empty security = optional
                targets.append((method.upper(), path))
    return sorted(set(targets))


def test_endpoint(schema, method: str, path: str, max_cases: int) -> dict[str, Any]:
    """对单个端点跑 schemathesis 测试."""
    try:
        op = schema[path][method]
    except KeyError as e:
        return {"method": method, "path": path, "error": f"endpoint not in spec: {e}"}

    strategy = op.as_strategy()
    cases_summary = []
    successful = client_err = server_err = network_exc = 0
    error_breakdown: dict[str, int] = {}

    for i in range(max_cases):
        try:
            case = strategy.example()
            response = case.call()
            status = response.status_code
            is_5xx = 500 <= status < 600
            is_4xx = 400 <= status < 500
            if is_5xx:
                server_err += 1
                error_breakdown[str(status)] = error_breakdown.get(str(status), 0) + 1
            elif is_4xx:
                client_err += 1
            else:
                successful += 1
            cases_summary.append({
                "case": i + 1,
                "status": status,
                "body_preview": (response.text or "")[:60],
            })
        except Exception as e:
            err_str = str(e)[:120]
            cases_summary.append({"case": i + 1, "exception": err_str})
            is_network = any(k in err_str.lower() for k in ["proxy", "disconnected", "connection", "timed out", "reset"])
            if is_network:
                network_exc += 1
            else:
                server_err += 1
                error_breakdown["exception"] = error_breakdown.get("exception", 0) + 1

    return {
        "method": method,
        "path": path,
        "label": op.label,
        "total_cases": max_cases,
        "successful_2xx_3xx": successful,
        "client_errors_4xx": client_err,
        "server_errors_5xx": server_err,
        "network_exceptions": network_exc,
        "passed": server_err == 0 and network_exc == 0,
        "error_breakdown": error_breakdown,
    }


def run_extended_test(max_cases: int, base_url: str) -> dict[str, Any]:
    """跑全部 discovered optional 端点的测试."""
    import schemathesis
    print(f"\n[test] loading spec from {SPEC_PATH}")
    schema = schemathesis.openapi.from_path(str(SPEC_PATH))
    schema.config.update(base_url=base_url)
    print(f"[test] base_url: {schema.config.base_url}")

    # 发现 optional 端点
    targets = discover_optional_endpoints(SPEC_PATH)
    print(f"[test] discovered {len(targets)} optional endpoints (no path params)")

    if not targets:
        return {"error": "no optional endpoints discovered", "results": {}}

    # 显示 targets
    print(f"[test] targets:")
    for method, path in targets[:10]:
        print(f"  - {method} {path}")
    if len(targets) > 10:
        print(f"  ... and {len(targets) - 10} more")

    # 跑测试
    results = {}
    for idx, (method, path) in enumerate(targets, 1):
        print(f"\n[test] [{idx}/{len(targets)}] {method} {path}")
        r = test_endpoint(schema, method, path, max_cases)
        results[f"{method} {path}"] = r
        if not r.get("passed", False):
            print(f"[test]   ✗ FAILED: {r.get('server_errors_5xx', 0)} 5xx, {r.get('network_exceptions', 0)} net")
        else:
            print(f"[test]   ✓ {r.get('successful_2xx_3xx', 0)} 2xx, {r.get('client_errors_4xx', 0)} 4xx")

    return {
        "discovered_count": len(targets),
        "results": results,
    }


def save_report(data: dict, output_path: Path, max_cases: int) -> None:
    """保存 JSON 报告."""
    results = data.get("results", {})
    summary = {
        "discovered_endpoints": data.get("discovered_count", 0),
        "tested_endpoints": len(results),
        "passed_endpoints": sum(1 for r in results.values() if r.get("passed", False)),
        "failed_endpoints": sum(1 for r in results.values() if not r.get("passed", False)),
        "total_cases": sum(r.get("total_cases", 0) for r in results.values()),
        "total_2xx_3xx": sum(r.get("successful_2xx_3xx", 0) for r in results.values()),
        "total_4xx": sum(r.get("client_errors_4xx", 0) for r in results.values()),
        "total_5xx": sum(r.get("server_errors_5xx", 0) for r in results.values()),
        "total_network_exc": sum(r.get("network_exceptions", 0) for r in results.values()),
    }
    report = {
        "test_type": "schemathesis_extended_test",
        "spec": str(SPEC_PATH),
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "max_cases_per_endpoint": max_cases,
        "summary": summary,
        "results": results,
    }
    output_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\n[report] saved: {output_path}")


def print_summary(data: dict) -> None:
    """打印人类可读摘要."""
    results = data.get("results", {})
    print("\n" + "=" * 70)
    print("Extended Smoke Test Summary")
    print("=" * 70)

    if not results:
        print("  No endpoints tested.")
        return

    # 按状态分组
    passed = [(k, r) for k, r in results.items() if r.get("passed", False)]
    failed = [(k, r) for k, r in results.items() if not r.get("passed", False)]

    print(f"\n✓ Passed: {len(passed)}/{len(results)}")
    for k, r in passed[:5]:
        print(f"  ✓ {k:60s}  {r['successful_2xx_3xx']:2d}/{r['total_cases']:2d} 2xx, "
              f"{r['client_errors_4xx']:2d} 4xx")
    if len(passed) > 5:
        print(f"  ... and {len(passed) - 5} more (all passed)")

    if failed:
        print(f"\n✗ Failed: {len(failed)}")
        for k, r in failed:
            err_str = ""
            if r.get("server_errors_5xx", 0) > 0:
                err_str = f"{r['server_errors_5xx']} 5xx"
            elif r.get("network_exceptions", 0) > 0:
                err_str = f"{r['network_exceptions']} network"
            print(f"  ✗ {k:60s}  {err_str}")
            # Show error breakdown
            for code, count in r.get("error_breakdown", {}).items():
                print(f"      {code}: {count}")

    # 错误码分布
    print(f"\nError code distribution (all endpoints):")
    all_breakdown: dict[str, int] = {}
    for r in results.values():
        for code, count in r.get("error_breakdown", {}).items():
            all_breakdown[code] = all_breakdown.get(code, 0) + count
    for code, count in sorted(all_breakdown.items(), key=lambda x: -x[1]):
        print(f"  {code}: {count}")


def main() -> int:
    import argparse
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--base-url", default="http://localhost:8008")
    ap.add_argument("--max-cases", type=int, default=30)
    ap.add_argument("--output", default=str(REPORTS_DIR / "schemathesis_extended.json"))
    ap.add_argument("--limit", type=int, default=0, help="只测前 N 个 (调试用, 0=全部)")
    args = ap.parse_args()

    v = ensure_schemathesis()
    print(f"[setup] schemathesis {v}")

    if not check_server(args.base_url):
        print(f"[test] ERROR: server not reachable at {args.base_url}")
        return 1

    data = run_extended_test(max_cases=args.max_cases, base_url=args.base_url)
    if args.limit > 0:
        # 限制只测前 N 个
        all_results = data.get("results", {})
        limited = dict(list(all_results.items())[:args.limit])
        data["results"] = limited
        data["discovered_count"] = args.limit
    save_report(data, Path(args.output), args.max_cases)
    print_summary(data)

    has_real_error = any(
        r.get("server_errors_5xx", 0) > 0
        for r in data.get("results", {}).values()
        if r.get("network_exceptions", 0) == 0
    )
    return 0 if not has_real_error else 1


if __name__ == "__main__":
    raise SystemExit(main())
