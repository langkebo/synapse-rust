#!/usr/bin/env python3
"""
schemathesis_authenticated_test.py — 带 user/admin token 的 schemathesis 冒烟测试
(Week 2 Task 2 + Task 3: errcode 规范校验)

覆盖:
  - 全部 user-auth 端点 (security: AccessToken): user token
  - 全部 admin 端点 (security: AccessToken + admin path): admin token
  - 不稳定 API (含 /unstable/) 暂不测,因 MSC 草稿 schema 变化大

每端点 10 个随机 case (减少 runtime),按 method 注入相应 token。

4xx errcode 规范校验 (Week 2 Task 3):
  - 按端点类型 (path 前缀 × method) 查 errcode_validator.py 规则
  - 不在白名单的 errcode 被标记为 unexpected
  - 汇总到最终报告的 errcode_validation 字段

输出: scripts/api_test/reports/schemathesis_authenticated.json

用法:
  python3 scripts/api_test/schemathesis_authenticated_test.py
  python3 scripts/api_test/schemathesis_authenticated_test.py --max-cases 20
  python3 scripts/api_test/schemathesis_authenticated_test.py --limit 10  # 调试: 只测前 10 端点
  python3 scripts/api_test/schemathesis_authenticated_test.py --no-errcode-check  # 跳过 errcode 校验
"""

from __future__ import annotations

import json
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

# 添加当前目录到 sys.path 以便导入 token_manager / errcode_validator
sys.path.insert(0, str(SCRIPT_DIR))
from token_manager import TokenManager  # noqa: E402
from errcode_validator import validate_errcode  # noqa: E402


def ensure_schemathesis() -> str:
    try:
        import schemathesis

        return schemathesis.__version__
    except ImportError:
        print("[setup] installing schemathesis ...")
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "pip",
                "install",
                "--quiet",
                "--user",
                "schemathesis",
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise RuntimeError(f"pip install failed: {result.stderr[:300]}")
        import schemathesis

        return schemathesis.__version__


def check_server(base_url: str) -> bool:
    import urllib.request

    try:
        req = urllib.request.Request(
            f"{base_url}/_matrix/client/r0/capabilities", method="GET"
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            return resp.status == 200
    except Exception:
        return False


def discover_authenticated_endpoints(spec_path: Path) -> list[tuple[str, str, str]]:
    """发现需要 auth 的端点.

    Returns:
        List of (method, path, auth_type) tuples
        auth_type: 'user' | 'admin'
    """
    import yaml

    spec = yaml.safe_load(spec_path.read_text(encoding="utf-8"))
    targets = []
    for path, methods in spec.get("paths", {}).items():
        if "{" in path:
            continue  # 过滤 path params
        if "/unstable/" in path:
            continue  # 过滤不稳定 API
        for method, op in methods.items():
            if method.lower() not in {"get", "post", "put", "delete", "patch"}:
                continue
            sec = op.get("security", [])
            if not sec:
                continue  # optional, 已在 Week 2 Task 1 测过
            is_admin = "/_synapse/admin/" in path
            is_user = any("AccessToken" in s for s in sec)
            if is_admin:
                targets.append((method.upper(), path, "admin"))
            elif is_user:
                targets.append((method.upper(), path, "user"))
    return sorted(set(targets))


def test_endpoint(
    schema,
    method: str,
    path: str,
    auth_type: str,
    max_cases: int,
    token_manager: TokenManager,
    enable_errcode_check: bool = True,
) -> dict[str, Any]:
    """对单个端点跑 schemathesis 测试,根据 auth_type 注入对应 token.

    收集每个 4xx 的 errcode 并按 errcode_validator 规则做规范校验.
    """
    try:
        op = schema[path][method]
    except KeyError as e:
        return {
            "method": method,
            "path": path,
            "auth_type": auth_type,
            "error": f"endpoint not in spec: {e}",
        }

    token = (
        token_manager.get_admin_token()
        if auth_type == "admin"
        else token_manager.get_user_token()
    )
    if not token:
        return {
            "method": method,
            "path": path,
            "auth_type": auth_type,
            "error": f"no {auth_type} token available",
        }

    strategy = op.as_strategy()
    cases_summary = []
    successful = client_err = server_err = network_exc = 0
    error_breakdown: dict[str, int] = {}

    # errcode 校验
    errcode_summary: dict[str, int] = {}
    unexpected_errcodes: dict[
        str, dict
    ] = {}  # errcode -> {count, reason, sample_status, sample_body}

    for i in range(max_cases):
        try:
            case = strategy.example()
            response = case.call(headers={"Authorization": f"Bearer {token}"})
            status = response.status_code
            is_5xx = 500 <= status < 600
            is_4xx = 400 <= status < 500
            body_text = response.text or ""
            # 收集 4xx errcode 详情
            errcode = None
            if is_4xx:
                try:
                    body_json = response.json()
                    errcode = body_json.get("errcode")
                except Exception:
                    errcode = "unparseable"
            if is_5xx:
                server_err += 1
                error_breakdown[str(status)] = error_breakdown.get(str(status), 0) + 1
            elif is_4xx:
                client_err += 1
                if errcode and errcode != "unparseable":
                    error_breakdown[f"errcode:{errcode}"] = (
                        error_breakdown.get(f"errcode:{errcode}", 0) + 1
                    )
                    errcode_summary[errcode] = errcode_summary.get(errcode, 0) + 1
                    # Week 2 Task 3: 按端点类型做 errcode 规范校验
                    # (unparseable 已由 validator 视为合法 — 它表示非 JSON 响应)
                    if enable_errcode_check:
                        v = validate_errcode(path, method, errcode)
                        if not v["valid"]:
                            prev = unexpected_errcodes.get(
                                errcode, {"count": 0, "reason": v["reason"]}
                            )
                            prev["count"] += 1
                            prev.setdefault("sample_status", status)
                            prev.setdefault("sample_body", body_text[:120])
                            unexpected_errcodes[errcode] = prev
                elif errcode == "unparseable":
                    error_breakdown["errcode:unparseable"] = (
                        error_breakdown.get("errcode:unparseable", 0) + 1
                    )
            else:
                successful += 1
            cases_summary.append(
                {
                    "case": i + 1,
                    "status": status,
                    "errcode": errcode,
                    "body_preview": body_text[:60],
                }
            )
        except Exception as e:
            err_str = str(e)[:120]
            cases_summary.append({"case": i + 1, "exception": err_str})
            is_network = any(
                k in err_str.lower()
                for k in ["proxy", "disconnected", "connection", "timed out", "reset"]
            )
            if is_network:
                network_exc += 1
            else:
                server_err += 1
                error_breakdown["exception"] = error_breakdown.get("exception", 0) + 1

    return {
        "method": method,
        "path": path,
        "auth_type": auth_type,
        "label": op.label,
        "total_cases": max_cases,
        "successful_2xx_3xx": successful,
        "client_errors_4xx": client_err,
        "server_errors_5xx": server_err,
        "network_exceptions": network_exc,
        "passed": server_err == 0 and network_exc == 0,
        "error_breakdown": error_breakdown,
        "errcode_summary": errcode_summary,
        "unexpected_errcodes": unexpected_errcodes,
        "errcode_validation_passed": len(unexpected_errcodes) == 0,
    }


def run_authenticated_test(
    max_cases: int,
    base_url: str,
    limit: int,
    token_manager: TokenManager,
    enable_errcode_check: bool = True,
) -> dict[str, Any]:
    """跑全部 discovered auth 端点."""
    import schemathesis

    print(f"\n[test] loading spec from {SPEC_PATH}")
    schema = schemathesis.openapi.from_path(str(SPEC_PATH))
    schema.config.update(base_url=base_url)
    print(f"[test] base_url: {schema.config.base_url}")

    targets = discover_authenticated_endpoints(SPEC_PATH)
    print(f"[test] discovered {len(targets)} authenticated endpoints (no path params)")

    # 按 auth_type 分组统计
    user_count = sum(1 for _, _, t in targets if t == "user")
    admin_count = sum(1 for _, _, t in targets if t == "admin")
    print(f"[test] user-auth: {user_count}, admin: {admin_count}")

    if limit > 0:
        targets = targets[:limit]
        print(f"[test] limited to first {limit} endpoints")

    results = {}
    for idx, (method, path, auth_type) in enumerate(targets, 1):
        if idx % 50 == 0 or idx <= 5:
            print(f"\n[test] [{idx}/{len(targets)}] {method} {path} (auth={auth_type})")
        r = test_endpoint(
            schema,
            method,
            path,
            auth_type,
            max_cases,
            token_manager,
            enable_errcode_check,
        )
        results[f"{method} {path}"] = r
        if idx % 50 == 0 or idx <= 5:
            if not r.get("passed", False):
                print(
                    f"[test]   ✗ FAILED: {r.get('server_errors_5xx', 0)} 5xx, {r.get('network_exceptions', 0)} net"
                )
            else:
                unexpected = r.get("unexpected_errcodes", {})
                if unexpected:
                    print(
                        f"[test]   ⚠ {r.get('successful_2xx_3xx', 0)} 2xx, {r.get('client_errors_4xx', 0)} 4xx, {len(unexpected)} unexpected errcode(s)"
                    )
                else:
                    print(
                        f"[test]   ✓ {r.get('successful_2xx_3xx', 0)} 2xx, {r.get('client_errors_4xx', 0)} 4xx"
                    )

    return {
        "discovered_count": len(targets),
        "user_count": user_count,
        "admin_count": admin_count,
        "errcode_check_enabled": enable_errcode_check,
        "results": results,
    }


def save_report(data: dict, output_path: Path, max_cases: int) -> None:
    """保存 JSON 报告."""
    results = data.get("results", {})
    # 聚合 errcode 校验结果
    endpoints_with_unexpected: dict[str, dict] = {}
    total_unexpected_errcode_cases = 0
    for k, r in results.items():
        unexpected = r.get("unexpected_errcodes", {})
        if unexpected:
            endpoints_with_unexpected[k] = {
                "method": r.get("method"),
                "path": r.get("path"),
                "auth_type": r.get("auth_type"),
                "unexpected": unexpected,
            }
            total_unexpected_errcode_cases += sum(
                u.get("count", 0) for u in unexpected.values()
            )

    summary = {
        "discovered_endpoints": data.get("discovered_count", 0),
        "user_endpoints": data.get("user_count", 0),
        "admin_endpoints": data.get("admin_count", 0),
        "tested_endpoints": len(results),
        "passed_endpoints": sum(1 for r in results.values() if r.get("passed", False)),
        "failed_endpoints": sum(
            1 for r in results.values() if not r.get("passed", False)
        ),
        "total_cases": sum(r.get("total_cases", 0) for r in results.values()),
        "total_2xx_3xx": sum(r.get("successful_2xx_3xx", 0) for r in results.values()),
        "total_4xx": sum(r.get("client_errors_4xx", 0) for r in results.values()),
        "total_5xx": sum(r.get("server_errors_5xx", 0) for r in results.values()),
        "total_network_exc": sum(
            r.get("network_exceptions", 0) for r in results.values()
        ),
        "errcode_check_enabled": data.get("errcode_check_enabled", True),
        "errcode_validation_passed_endpoints": sum(
            1 for r in results.values() if r.get("errcode_validation_passed", True)
        ),
        "errcode_validation_failed_endpoints": len(endpoints_with_unexpected),
        "total_unexpected_errcode_cases": total_unexpected_errcode_cases,
    }
    report = {
        "test_type": "schemathesis_authenticated_test",
        "spec": str(SPEC_PATH),
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "max_cases_per_endpoint": max_cases,
        "summary": summary,
        "results": results,
        "errcode_violations": endpoints_with_unexpected,
    }
    output_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    print(f"\n[report] saved: {output_path}")


def print_summary(data: dict) -> None:
    """打印摘要."""
    results = data.get("results", {})
    print("\n" + "=" * 70)
    print("Authenticated Smoke Test Summary")
    print("=" * 70)
    print(
        f"Discovered: {data.get('discovered_count', 0)} endpoints "
        f"({data.get('user_count', 0)} user + {data.get('admin_count', 0)} admin)"
    )

    if not results:
        print("  No endpoints tested.")
        return

    passed = [(k, r) for k, r in results.items() if r.get("passed", False)]
    failed = [(k, r) for k, r in results.items() if not r.get("passed", False)]

    print(f"\n✓ Passed: {len(passed)}/{len(results)}")
    print(f"✗ Failed: {len(failed)}/{len(results)}")

    if failed:
        print(f"\nFailed endpoints (first 10):")
        for k, r in failed[:10]:
            err_str = ""
            if r.get("server_errors_5xx", 0) > 0:
                err_str = f"{r['server_errors_5xx']} 5xx"
            elif r.get("network_exceptions", 0) > 0:
                err_str = f"{r['network_exceptions']} network"
            auth = r.get("auth_type", "?")
            print(f"  ✗ [{auth:5s}] {k:60s}  {err_str}")

    # Error breakdown
    all_breakdown: dict[str, int] = {}
    for r in results.values():
        for code, count in r.get("error_breakdown", {}).items():
            all_breakdown[code] = all_breakdown.get(code, 0) + count
    if all_breakdown:
        print(f"\nError code distribution:")
        for code, count in sorted(all_breakdown.items(), key=lambda x: -x[1]):
            print(f"  {code}: {count}")

    # Week 2 Task 3: errcode 校验结果
    if data.get("errcode_check_enabled", True):
        endpoints_with_unexpected = [
            (k, r) for k, r in results.items() if r.get("unexpected_errcodes")
        ]
        if endpoints_with_unexpected:
            print(
                f"\n⚠ Errcode Validation: {len(endpoints_with_unexpected)}/{len(results)} endpoints returned unexpected errcodes"
            )
            for k, r in endpoints_with_unexpected[:10]:
                unexpected = r["unexpected_errcodes"]
                codes = list(unexpected.keys())
                print(f"  ⚠ {k}")
                for ec, info in unexpected.items():
                    print(f"     {ec}: {info['count']} cases — {info['reason']}")
        else:
            print(f"\n✓ Errcode Validation: all endpoints returned expected errcodes")


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--base-url", default="http://localhost:8008")
    ap.add_argument("--max-cases", type=int, default=10)
    ap.add_argument(
        "--output", default=str(REPORTS_DIR / "schemathesis_authenticated.json")
    )
    ap.add_argument("--limit", type=int, default=0, help="只测前 N 个 (调试用, 0=全部)")
    ap.add_argument(
        "--config",
        default=str(SCRIPT_DIR / "config.yaml"),
        help="token_manager 配置文件",
    )
    ap.add_argument(
        "--no-errcode-check",
        action="store_true",
        help="跳过 errcode 规范校验 (Week 2 Task 3)",
    )
    args = ap.parse_args()

    v = ensure_schemathesis()
    print(f"[setup] schemathesis {v}")

    if not check_server(args.base_url):
        print(f"[test] ERROR: server not reachable at {args.base_url}")
        return 1

    token_manager = TokenManager(base_url=args.base_url, config_path=args.config)
    user_token = token_manager.get_user_token()
    admin_token = token_manager.get_admin_token()
    if not user_token or not admin_token:
        print(
            f"[test] ERROR: failed to get tokens (user={bool(user_token)}, admin={bool(admin_token)})"
        )
        return 1

    enable_errcode_check = not args.no_errcode_check
    data = run_authenticated_test(
        max_cases=args.max_cases,
        base_url=args.base_url,
        limit=args.limit,
        token_manager=token_manager,
        enable_errcode_check=enable_errcode_check,
    )
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
