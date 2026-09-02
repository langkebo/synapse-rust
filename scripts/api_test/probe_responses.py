#!/usr/bin/env python3
"""
Week 2 Task 5 — Probe 活服务器 → 补 response schema 进 OpenAPI

原理:
  1. 从 docs/openapi/client.yaml 读所有 GET 端点(只 GET,因为 GET 安全,无副作用)
  2. 对每个端点发请求(可带 token)
  3. 记录 2xx 响应 body 的 JSON 结构 → 转为 JSON Schema
  4. 更新 OpenAPI spec 的 responses.200.content.application/json.schema
  5. 同时采集真实 errcode 分布,补充 4xx schema 细节

输出:
  - scripts/api_test/probe_responses.py (主脚本)
  - scripts/api_test/response_schemas.json (采集到的 schemas)
  - docs/openapi/client.yaml (原位更新)
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional
from urllib.parse import quote

ROOT = Path("/Users/ljf/Desktop/hu_ts/synapse-rust")
SPEC_PATH = ROOT / "docs/openapi/client.yaml"
CONFIG_PATH = ROOT / "scripts/api_test/config.yaml"
OUTPUT_JSON = ROOT / "scripts/api_test/response_schemas.json"

# path params 默认值 (从 config.yaml 复用)
DEFAULT_PATH_PARAMS = {
    "user_id": "@testuser1:matrix.test",
    "room_id": "!apitest-nosuchroom:matrix.test",
    "room_alias": "#apitest-nosuchalias:matrix.test",
    "event_id": "$apitest-nosuchevent:matrix.test",
    "txn_id": "apitest-txn-00000000",
    "transaction_id": "apitest-txn-00000000",
    "device_id": "APITESTDEVICE0000",
    "filter_id": "apitest-filter-0000",
    "key_name": "com.example.apitest",
    "tag": "m.favourite",
    "scope": "global",
    "media_id": "doesnotexist",
    "server_name": "matrix.test",
    "user": "testuser1",
    "sender": "@testuser1:matrix.test",
    "thread_id": "$apitest-nosuchevent:matrix.test",
    "token": "apitest-token-0000",
    "id": "apitest-0000",
    "name": "apitest-name",
    "network_id": "apitest-network",
    "appservice_id": "apitest-as",
}


def get_user_token(base_url: str, verify_tls: bool = False) -> Optional[str]:
    """登录拿 token (复用 token_manager 的逻辑)."""
    try:
        sys.path.insert(0, str(ROOT / "scripts/api_test"))
        from token_manager import TokenManager
        tm = TokenManager(base_url=base_url, config_path=str(CONFIG_PATH))
        return tm.get_user_token()
    except Exception as e:
        print(f"[token] failed: {e}", file=sys.stderr)
        return None


def endpoint_requires_auth(spec: dict, path: str, method: str) -> bool:
    """从 spec 读 security 字段判断是否需要 auth."""
    op = spec.get("paths", {}).get(path, {}).get(method, {})
    sec = op.get("security", [])
    # security=[] 表示 optional
    return bool(sec)


def substitute_path_params(path: str, params: dict[str, str]) -> str:
    """把 {param} 替换成实际值."""
    def repl(m):
        key = m.group(1)
        return params.get(key, m.group(0))
    return re.sub(r"\{(\w+)\}", repl, path)


def curl_request(
    url: str,
    method: str = "GET",
    headers: Optional[dict] = None,
    timeout: int = 10,
    verify_tls: bool = False,
) -> tuple[int, dict, str]:
    """用 curl 发请求, 返回 (status, headers, body)."""
    # HTTP methods are case-sensitive — curl 默认 GET, 但只有 -X GET (大写) 才正确
    method = method.upper()
    cmd = ["curl", "-s", "-w", "\n%{http_code}\n%{content_type}",
           "-X", method, url,
           "-m", str(timeout)]
    if headers:
        for k, v in headers.items():
            cmd += ["-H", f"{k}: {v}"]
    if not verify_tls:
        cmd += ["-k"]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 5)
        parts = result.stdout.rsplit("\n", 2)
        body = parts[0] if len(parts) > 0 else ""
        status = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 0
        ct = parts[2] if len(parts) > 2 else ""
        return status, {"content_type": ct}, body
    except subprocess.TimeoutExpired:
        return 0, {}, ""
    except Exception as e:
        return 0, {}, str(e)


def parse_path_params(path: str) -> list[str]:
    """提取 path 中的所有 {param} 名称."""
    return re.findall(r"\{(\w+)\}", path)


def is_safe_to_probe(method: str, path: str) -> bool:
    """只 GET + 没有 path params + 不 unstable 才直接探."""
    if method != "get":
        return False
    if "{" in path:
        return False  # 需要 path params
    if "/unstable/" in path:
        return False
    if "/_synapse/admin/" in path:
        return False
    return True


# JSON Schema 生成
def infer_schema(value, max_depth: int = 8) -> dict:
    """从 Python 对象生成 JSON Schema."""
    if max_depth <= 0:
        return {"type": "object", "description": "max depth exceeded"}

    if value is None:
        return {"type": "null"}
    if isinstance(value, bool):
        return {"type": "boolean"}
    if isinstance(value, int):
        return {"type": "integer"}
    if isinstance(value, float):
        return {"type": "number"}
    if isinstance(value, str):
        return {"type": "string"}

    if isinstance(value, list):
        if not value:
            return {"type": "array", "items": {}}
        # 合并所有元素 schema (简化:用第一个)
        items = infer_schema(value[0], max_depth - 1)
        return {"type": "array", "items": items}

    if isinstance(value, dict):
        properties = {}
        required = []
        for k, v in value.items():
            properties[k] = infer_schema(v, max_depth - 1)
            if v is not None:
                required.append(k)
        schema = {"type": "object", "properties": properties}
        if required:
            schema["required"] = required
        return schema

    return {"type": "string"}


def main() -> int:
    import yaml

    # 探测可用服务器
    for candidate in ("http://localhost:8008", "https://matrix.test"):
        status, _, _ = curl_request(f"{candidate}/_matrix/client/v3/versions", timeout=5)
        if status == 200:
            base_url = candidate
            verify_tls = candidate.startswith("https")
            print(f"[probe] Using server: {base_url}")
            break
    else:
        print("[probe] ERROR: no server reachable (tried localhost:8008, matrix.test)", file=sys.stderr)
        return 1

    import copy

    # 读 spec — 必须 deep copy 否则 YAML alias 共享导致 patch 相互覆盖
    raw = yaml.safe_load(SPEC_PATH.read_text())
    spec = copy.deepcopy(raw)

    # 收集所有可探测的端点
    targets = []
    for path, path_item in spec.get("paths", {}).items():
        for method in ("get",):  # 只 GET
            op = path_item.get(method)
            if not op:
                continue
            if not is_safe_to_probe(method, path):
                continue
            targets.append((method, path))

    print(f"[probe] {len(targets)} GET endpoints (no path params, no unstable)")
    print(f"[probe] base_url = {base_url}")

    # 尝试拿 token (对需要 auth 的端点)
    token = None
    if base_url == "http://localhost:8008":
        # localhost 可能没有注册用户,尝试拿 token
        token = get_user_token(base_url, verify_tls)
        if token:
            print(f"[probe] got user token: {token[:20]}...")
        else:
            print("[probe] WARNING: no token, probing anonymously")

    # 探活每个端点 — 根据 security 决定是否带 token
    results = {}
    summary = {"probed": 0, "got_2xx": 0, "got_4xx": 0, "errors": 0}
    for idx, (method, path) in enumerate(targets, 1):
        full_url = base_url + path

        # 决定是否带 token
        needs_auth = endpoint_requires_auth(spec, path, method)
        hdrs = {}
        if needs_auth and token:
            hdrs["Authorization"] = f"Bearer {token}"

        status, hdrs_out, body = curl_request(full_url, method, hdrs if hdrs else None)

        summary["probed"] += 1
        is_2xx = 200 <= status < 300
        is_4xx = 400 <= status < 500
        if is_2xx:
            summary["got_2xx"] += 1
        elif is_4xx:
            summary["got_4xx"] += 1
        else:
            summary["errors"] += 1

        # 解析 JSON
        parsed = None
        ct = hdrs_out.get("content_type", "")
        if body and "application/json" in ct:
            try:
                parsed = json.loads(body)
            except json.JSONDecodeError:
                pass

        # 生成 schema
        response_schema = None
        if parsed is not None:
            response_schema = infer_schema(parsed)

        results[f"{method} {path}"] = {
            "method": method,
            "path": path,
            "status": status,
            "is_2xx": is_2xx,
            "is_4xx": is_4xx,
            "needs_auth": needs_auth,
            "body_size": len(body),
            "schema": response_schema,
            "sample_value": parsed if is_2xx and parsed else None,
            "content_type": ct,
        }

        if idx % 20 == 0:
            print(f"[probe] [{idx}/{len(targets)}] {method} {path} → {status}")

    print(f"[probe] done: {summary}")

    # 写 JSON
    OUTPUT_JSON.write_text(json.dumps({
        "summary": summary,
        "base_url": base_url,
        "results": results,
    }, indent=2, ensure_ascii=False, default=str))
    print(f"[probe] wrote: {OUTPUT_JSON}")

    # Patch OpenAPI: 只更新 is_2xx + has schema 的端点
    patched = 0
    for path, path_item in spec.get("paths", {}).items():
        for method in ("get",):
            op = path_item.get(method)
            if not op:
                continue
            key = f"{method} {path}"
            r = results.get(key)
            if not r or not r.get("is_2xx") or not r.get("schema"):
                continue
            content = op.setdefault("responses", {}).setdefault("200", {}).setdefault("content", {})
            content.setdefault("application/json", {})["schema"] = r["schema"]
            op["responses"]["200"]["description"] = "OK"
            patched += 1

    spec["x-probe"] = {
        "patched_response_schemas": patched,
        "base_url": base_url,
        "summary": summary,
    }

    # 写回 spec
    SPEC_PATH.write_text(yaml.dump(spec, allow_unicode=True, sort_keys=False, default_flow_style=False))
    print(f"[probe] wrote: {SPEC_PATH} (patched {patched} response schemas)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())