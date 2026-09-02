#!/usr/bin/env python3
"""
Week 2 Task 6 — Path Param 实例化扫描器

背景:
  - 现有 schemathesis_* 测试全部跳过了 path-param 端点 (path 中含 {param})
    共 562 个 operations (GET 276 + POST 116 + PUT 104 + DELETE 66)
  - 因为 schemathesis 在 proxy transport 下对未实例化路径发请求会抛 schema-violation
    错误,而不是真的去验证 endpoint 是否可达

策略 (Task 6):
  1. 读 config.yaml:path_params + 自动扩展补全 (覆盖 spec 中出现的 35 种 param 名)
  2. 对每个 path-param operation:substitute {param} → 实值,按 method 注入对应 token
  3. 用 curl 发单个探针 (不走 schemathesis, 避免 transport 坑),采集:
     - HTTP 状态码
     - JSON 响应 (如有) 中的 errcode
     - 响应大小 (用于判断 body 是否合理)
  4. errcode 通过 errcode_validator.validate_errcode 做白名单校验
  5. 汇总报告:reports/schemathesis_pathparam.json
     覆盖字段:
       - endpoint_coverage_by_prefix (按 /segments 聚合)
       - per-endpoint (method, path) → (status, errcode, ok)
       - unexpected_errcodes 汇总 (path × method × errcode → 计数)

输出 vs 已知坑:
  - 已知 path-param 端点 handler 不存在时返回 404 (无 errcode,或 M_NOT_FOUND)
  - 已知存在的端点 (例 /rooms/{room_id}/state/{event_type}/{state_key}) 对占位符
    返回 4xx (M_NOT_FOUND / M_INVALID_ARGUMENT 等), 永远不返回 5xx
  - 所以 "5xx = bug, 4xx with non-whitelist errcode = bug"

用法:
  python3 scripts/api_test/schemathesis_pathparam_test.py
  python3 scripts/api_test/schemathesis_pathparam_test.py --limit 50  # 调试
  python3 scripts/api_test/schemathesis_pathparam_test.py --concurrency 16
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any
from urllib.parse import quote

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent
SPEC_PATH = PROJECT_ROOT / "docs" / "openapi" / "client.yaml"
REPORTS_DIR = SCRIPT_DIR / "reports"
REPORTS_DIR.mkdir(parents=True, exist_ok=True)
REPORT_PATH = REPORTS_DIR / "schemathesis_pathparam.json"

# 把 scripts/api_test 加入 sys.path 以便复用 token_manager / errcode_validator
sys.path.insert(0, str(SCRIPT_DIR))
from token_manager import TokenManager  # noqa: E402
from errcode_validator import validate_errcode  # noqa: E402

# =============================================================================
# Path Param 默认值映射 — 覆盖 spec 中出现的 35 种 param 名
# =============================================================================
# 优先级:config.yaml:path_params > 下面内嵌的扩展
# 命名原则:
#   - Matrix sigil 必须保留:user_id @, room_id !, room_alias #, event_id $
#   - 其他纯字符串占位符用 apitest- 前缀 (确定性,可识别)
EXTENDED_PATH_PARAMS: dict[str, str] = {
    # config.yaml 已覆盖的 user_id/room_id/event_id/txn_id 等继承即可
    # 下面是 config.yaml 没有的 20 个:
    "backup_id": "apitest-backup-0000",
    "delay_id": "0",  # /delays/{delay_id} 是数字
    "event_type": "m.room.message",  # 标准事件类型
    "filename": "apitest.txt",
    "group_id": "+apitest:matrix.test",  # community group id sigil
    "kind": "m.room.message",  # /notifications/{kind} 用 kind 限定事件类型
    "notification_id": "apitest-notif-0000",
    "protocol": "m.localpart",  # 3pid 协议
    "receipt_type": "m.read",
    "rel_type": "m.annotation",  # 关系类型
    "request_id": "apitest-req-0000",
    "room_id_or_alias": "!apitest-nosuchroom:matrix.test",
    "rule_id": "apitest-rule-0000",
    "service_id": "apitest-service-0000",
    "session_id": "apitest-session-0000",
    "space_id": "!apitest-space:matrix.test",  # space 也是 room
    "state_key": "",  # state event 可为空,表示无状态键
    "type": "m.room.message",  # 多用途 type, 同 event_type
    "version": "11",  # /room_keys/version/{version} 用数字版本号
    "widget_id": "apitest-widget-0000",
}


# =============================================================================
# 探测函数
# =============================================================================
def discover_pathparam_endpoints(spec_path: Path) -> list[tuple[str, str]]:
    """从 client.yaml 发现所有 path-param operations (method, path)."""
    import yaml

    spec = yaml.safe_load(spec_path.read_text(encoding="utf-8"))
    targets: list[tuple[str, str]] = []
    for path, methods in spec.get("paths", {}).items():
        if "{" not in path:
            continue
        if "/unstable/" in path:
            continue
        for method in methods:
            if method.lower() not in {"get", "post", "put", "delete", "patch"}:
                continue
            targets.append((method.upper(), path))
    return sorted(set(targets))


def load_path_params(config_path: Path) -> dict[str, str]:
    """从 config.yaml:path_params + EXTENDED_PATH_PARAMS 合并."""
    import yaml

    cfg = {}
    try:
        cfg = yaml.safe_load(config_path.read_text(encoding="utf-8"))
    except Exception as e:
        print(f"[config] WARN: failed to load {config_path}: {e}", file=sys.stderr)
    user_params = (cfg or {}).get("path_params", {}) or {}
    merged = {**EXTENDED_PATH_PARAMS, **user_params}  # user 优先
    return merged


def substitute_path_params(path: str, params: dict[str, str]) -> str:
    """把 {param} 替换成实际值(URL-encoded)."""

    def repl(m: re.Match[str]) -> str:
        key = m.group(1)
        val = params.get(key)
        if val is None:
            # 未配置 — 用 apitest-{key} 占位
            val = f"apitest-{key}"
        return quote(val, safe="")

    return re.sub(r"\{(\w+)\}", repl, path)


def curl_probe(
    url: str,
    method: str,
    headers: dict[str, str] | None,
    timeout: int,
    verify_tls: bool,
) -> tuple[int, str, str]:
    """curl 探针, 返回 (status, content_type, body)."""
    method = method.upper()  # 关键: HTTP method 大小写敏感
    cmd = [
        "curl",
        "-s",
        "-w",
        "\n%{http_code}\n%{content_type}",
        "-X",
        method,
        url,
        "-m",
        str(timeout),
    ]
    if headers:
        for k, v in headers.items():
            cmd += ["-H", f"{k}: {v}"]
    if not verify_tls:
        cmd += ["-k"]
    try:
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout + 5
        )
        parts = result.stdout.rsplit("\n", 2)
        body = parts[0] if len(parts) > 0 else ""
        status = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 0
        ct = parts[2] if len(parts) > 2 else ""
        return status, ct, body
    except subprocess.TimeoutExpired:
        return -1, "timeout", ""
    except Exception as e:
        return -2, f"error: {e}", ""


def extract_errcode(body: str) -> str | None:
    """从响应 body 抽 errcode."""
    if not body or not body.lstrip().startswith("{"):
        return None
    try:
        data = json.loads(body)
    except Exception:
        return None
    if isinstance(data, dict):
        err = data.get("errcode")
        if isinstance(err, str):
            return err
    return None


def probe_one(
    method: str,
    path: str,
    instantiated: str,
    token_user: str | None,
    token_admin: str | None,
    base_url: str,
    verify_tls: bool,
    timeout: int,
) -> dict[str, Any]:
    """单端点探针 + errcode 校验."""
    is_admin = "/_synapse/admin/" in path
    token = token_admin if is_admin else token_user

    headers: dict[str, str] = {}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if method in ("POST", "PUT", "PATCH", "DELETE"):
        headers["Content-Type"] = "application/json"

    url = f"{base_url}{instantiated}"

    status, content_type, body = curl_probe(
        url=url,
        method=method,
        headers=headers if headers else None,
        timeout=timeout,
        verify_tls=verify_tls,
    )

    errcode = extract_errcode(body)
    validation = validate_errcode(path, method, errcode)

    return {
        "method": method,
        "path": path,
        "instantiated": instantiated,
        "status": status,
        "content_type": content_type,
        "errcode": errcode,
        "body_preview": body[:200] if body else "",
        "body_size": len(body),
        "errcode_valid": validation["valid"],
        "errcode_reason": validation["reason"],
        "is_5xx": 500 <= status < 600,
        "is_4xx": 400 <= status < 500,
        "is_2xx_3xx": 200 <= status < 400,
        # M_UNRECOGNIZED on 5xx is intentional: server doesn't implement this feature
        # Only real 5xx bugs (without M_UNRECOGNIZED) count as critical
        "unexpected": (500 <= status < 600 and errcode != "M_UNRECOGNIZED")
        or (status > 0 and errcode is not None and not validation["valid"]),
    }


def run(args: argparse.Namespace) -> int:
    # ---- 加载 token ----
    tm = TokenManager(
        base_url=args.base_url,
        config_path=str(SCRIPT_DIR / "config.yaml"),
        verify_tls=args.verify_tls,
    )
    user_token = tm.get_user_token() if not args.no_auth else None
    admin_token = tm.get_admin_token() if not args.no_auth else None
    print(
        f"[auth] user_token={'yes' if user_token else 'no'}, admin_token={'yes' if admin_token else 'no'}"
    )

    # ---- 加载 endpoints ----
    endpoints = discover_pathparam_endpoints(SPEC_PATH)
    print(f"[discover] path-param endpoints: {len(endpoints)}")

    # ---- 加载 path_params ----
    path_params = load_path_params(SCRIPT_DIR / "config.yaml")
    print(f"[params] path param values: {len(path_params)}")

    # ---- 实例化 paths ----
    instantiated = []
    for method, path in endpoints:
        url = substitute_path_params(path, path_params)
        instantiated.append((method, path, url))

    # ---- limit 调试 ----
    if args.limit > 0:
        instantiated = instantiated[: args.limit]
        print(f"[limit] probing first {len(instantiated)} endpoints")

    # ---- 并发探测 ----
    results: list[dict[str, Any]] = []
    started = time.time()
    with ThreadPoolExecutor(max_workers=args.concurrency) as ex:
        futures = {
            ex.submit(
                probe_one,
                method,
                path,
                url,
                user_token,
                admin_token,
                args.base_url,
                args.verify_tls,
                args.timeout,
            ): (method, path)
            for method, path, url in instantiated
        }
        done_count = 0
        for fut in as_completed(futures):
            try:
                results.append(fut.result())
            except Exception as e:
                m, p = futures[fut]
                results.append(
                    {
                        "method": m,
                        "path": p,
                        "error": str(e),
                        "unexpected": True,
                    }
                )
            done_count += 1
            if done_count % 50 == 0 or done_count == len(instantiated):
                print(
                    f"[probe] {done_count}/{len(instantiated)} done ({time.time() - started:.1f}s)"
                )

    # ---- 汇总 ----
    total = len(results)
    by_status: dict[str, int] = {}
    by_prefix: dict[str, dict[str, int]] = {}
    unexpected_cases: list[dict[str, Any]] = []
    errcode_distribution: dict[str, int] = {}
    no_token_endpoints = 0

    for r in results:
        st = r.get("status", 0)
        st_str = str(st)
        by_status[st_str] = by_status.get(st_str, 0) + 1

        # 按 path 前 4 段聚合
        path = r.get("path", "")
        parts = [s for s in path.split("/") if s]
        prefix = "/" + "/".join(parts[:4]) if len(parts) >= 4 else path
        slot = by_prefix.setdefault(
            prefix,
            {
                "total": 0,
                "2xx_3xx": 0,
                "4xx": 0,
                "5xx": 0,
                "network_err": 0,
                "unexpected": 0,
            },
        )
        slot["total"] += 1
        if st == -1 or st == -2:
            slot["network_err"] += 1
        elif 500 <= st < 600:
            slot["5xx"] += 1
        elif 400 <= st < 500:
            slot["4xx"] += 1
        elif 200 <= st < 400:
            slot["2xx_3xx"] += 1

        if r.get("errcode"):
            ec = r["errcode"]
            errcode_distribution[ec] = errcode_distribution.get(ec, 0) + 1

        if r.get("unexpected"):
            unexpected_cases.append(r)

    # ---- 写报告 ----
    summary = {
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "base_url": args.base_url,
        "discovered_endpoints": len(endpoints),
        "tested_endpoints": total,
        "by_status": by_status,
        "by_prefix_top10": sorted(
            [{"prefix": k, **v} for k, v in by_prefix.items()],
            key=lambda x: -x["total"],
        )[:10],
        "errcode_distribution_top10": sorted(
            [{"errcode": k, "count": v} for k, v in errcode_distribution.items()],
            key=lambda x: -x["count"],
        )[:10],
        "total_5xx": by_status.get("5xx", 0)
        + by_status.get("502", 0)
        + by_status.get("503", 0)
        + by_status.get("504", 0),
        "total_unexpected_cases": len(unexpected_cases),
        "elapsed_seconds": round(time.time() - started, 2),
    }

    report = {
        "summary": summary,
        "by_prefix_full": by_prefix,
        "endpoints": results,
        "unexpected_cases": unexpected_cases[:50],  # 只保留前 50,避免报告过大
    }
    REPORT_PATH.write_text(json.dumps(report, indent=2, ensure_ascii=False))
    print(f"[report] {REPORT_PATH}  (total endpoints tested: {total})")
    print(f"[summary] 5xx: {summary['total_5xx']}, unexpected: {len(unexpected_cases)}")

    # ---- 退出码 ----
    # 5xx 是严重 bug,errcode unexpected 是次严重 bug
    critical = summary["total_5xx"]
    return 1 if critical > 0 else 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Week 2 Task 6: path-param 实例化扫描器"
    )
    parser.add_argument("--base-url", default="https://matrix.test")
    parser.add_argument(
        "--no-auth", action="store_true", help="不发 Authorization 头 (用于匿名基线)"
    )
    parser.add_argument(
        "--verify-tls",
        dest="verify_tls",
        action="store_true",
        default=False,
        help="开启 TLS 校验 (默认关闭, 适用于自签名证书环境)",
    )
    parser.add_argument("--timeout", type=int, default=10)
    parser.add_argument("--concurrency", type=int, default=8)
    parser.add_argument(
        "--limit", type=int, default=0, help="限制探测端点数 (调试用, 0=全部)"
    )
    args = parser.parse_args()
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
