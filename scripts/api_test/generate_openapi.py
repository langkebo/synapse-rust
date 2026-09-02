#!/usr/bin/env python3
"""
generate_openapi.py — 从 RouteLedger JSON 产物生成 OpenAPI 3.0 规范

来源: scripts/api_test/ledger.json (synapse_ledger_export binary 输出)
目标: docs/openapi/client.yaml (仅 Client-Server API)

策略 (策略 A — 零侵入,只读 ledger):
  1. 解析 ledger.json 的 entry 列表
  2. 过滤出 `/_matrix/client/` 前缀的端点
  3. 归并 r0/v3 等版本变体到同一个 operationId
  4. 把 Axum 风格路径模板 `/{foo}` 转 OpenAPI `{foo}`
  5. 用 path_params / query_params / auth 字段填充 parameters
  6. 响应体 schema 留 TODO,只声明常见 200/204/400/401/403/404/429

输出结构:
  openapi: 3.0.3
  info: { title, version, description }
  servers: [{ url: http://localhost:8008 }]
  paths: { ... }
  components: { securitySchemes, schemas (placeholders) }

Usage:
  python3 scripts/api_test/generate_openapi.py \
      --ledger scripts/api_test/ledger.json \
      --output docs/openapi/client.yaml

CI 集成 (Week 1 Task 1 完成态):
  - 每次构建后跑此脚本 → 与上一次的 git diff 对比
  - 差异大于 5% → 提醒 "API 表面变更,需 review"
  - 差异 = 0 → 通过
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

# OpenAPI 3.0.3 标准状态码 schema (Minimal — 后续可扩)
DEFAULT_RESPONSES: dict[str, dict[str, Any]] = {
    "200": {"description": "OK", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/GenericResponse"}}}},
    "204": {"description": "No Content"},
    "400": {"description": "Bad Request", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/MatrixError"}}}},
    "401": {"description": "Unauthorized", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/MatrixError"}}}},
    "403": {"description": "Forbidden", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/MatrixError"}}}},
    "404": {"description": "Not Found", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/MatrixError"}}}},
    "429": {"description": "Too Many Requests", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/MatrixError"}}}},
}

# Matrix 错误响应 (Spec §5.1 — errcode/error)
MATRIX_ERROR_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": ["errcode", "error"],
    "properties": {
        "errcode": {"type": "string", "description": "Matrix error code, e.g. M_FORBIDDEN"},
        "error": {"type": "string", "description": "Human-readable error message"},
    },
    "additionalProperties": True,
    "example": {"errcode": "M_UNKNOWN", "error": "No known endpoint"},
}

GENERIC_RESPONSE_SCHEMA: dict[str, Any] = {
    "type": "object",
    "additionalProperties": True,
    "description": "TODO: Per-endpoint response schema will be filled in subsequent iterations (Week 1+2).",
}

# Path templates that capture user_id, room_id etc. — types inferred from param name
PATH_PARAM_TYPES: dict[str, str] = {
    "user_id": "string",
    "room_id": "string",
    "room_alias": "string",
    "device_id": "string",
    "session_id": "string",
    "txn_id": "string",
    "event_id": "string",
    "key_name": "string",
    "call_id": "string",
    "worker_id": "string",
    "rule_id": "string",
    "tag": "string",
    "filter_id": "string",
    "group_id": "string",
    "thread_id": "string",
    "relation_type": "string",
    "event_type": "string",
    "third_party_id": "string",
    "medium": "string",
    "address": "string",
}

# Auth requirements from RouteLedger.auth field → OpenAPI security
AUTH_TO_SECURITY: dict[str, list[dict[str, list[str]]]] = {
    "user": [{"AccessToken": []}],
    "admin": [{"AccessToken": []}],  # Synapse: admin paths require user token with admin flag
    "federation": [{"X-Matrix": []}],
    "optional": [],  # OpenAPI: omit security (公开端点)
    "none": [],
}

# ─────────────────────────────────────────────────────────────────────────────
# Auth heuristics (Strategy C — 路径/模块名推断 + ledger 优先)
# 覆盖 ledger.auth MISSING 的 897 个端点
# ─────────────────────────────────────────────────────────────────────────────
_AUTH_HEURISTICS: list[tuple[str, str, str]] = [
    # (pattern_type, pattern, inferred_auth)
    # 公开端点 — 任何人可访问 (anonymous OK)
    # 注意: 路径变体 ① `/_matrix/client/versions` (无前缀) ② `/_matrix/client/{r0,v1,v3,unstable/...}/versions`
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?login(?:/|$)", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?register(?:/|$)", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?account/3pid/email/requestToken$", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?account/3pid/email/submitToken$", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?account/password/email/requestToken$", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?account/password/email/submitToken$", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?capabilities(?:/|$)", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?versions$", "optional"),
    (r"path", r"^/_\.well-known/matrix/client$", "optional"),
    (r"path", r"^/_\.well-known/matrix/server$", "optional"),
    (r"path", r"^/_\.well-known/matrix/support$", "optional"),
    (r"path", r"^/_\.well-known/openid-configuration$", "optional"),
    (r"path", r"^/_\.well-known/jwks\.json$", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?publicRooms(?:/|$)", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?profile/{user_id}(?:/|$)", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?thirdparty/location(?:/|$)", "optional"),
    (r"path", r"^/_matrix/client/(?:(?:r0|v\d+|unstable)/)?thirdparty/protocol/(?:[A-Za-z0-9_.-]+)$", "optional"),
    # Admin 端点
    (r"path", r"^/_synapse/admin/", "admin"),
    # Federation 端点
    (r"path", r"^/_matrix/federation/", "federation"),
    (r"module", r"^federation::", "federation"),
    # Worker 端点 (worker profile 才会启用)
    (r"path", r"^/_synapse/worker/", "user"),
    (r"module", r"::worker::", "user"),
    # SAML OIDC CAS 认证入口 (未登录访问)
    (r"module", r"::saml::", "optional"),
    (r"module", r"::oidc::", "optional"),
    (r"module", r"::cas::", "optional"),
    # 公共 fallback — 大多数端点需要 user auth
    (r"default", r"", "user"),
]

import re as _re

# 缓存 compiled regex
_AUTH_PATTERN_CACHE: dict[str, tuple[str, str, str]] = {}
for _ptype, _pat, _auth in _AUTH_HEURISTICS:
    _AUTH_PATTERN_CACHE[f"{_ptype}:{_pat}"] = (_ptype, _pat, _auth)


def infer_auth(entry: dict[str, Any]) -> str | None:
    """从路径和模块名推断认证类型。ledger.auth 字段优先。"""
    explicit = entry.get("auth")
    if explicit:
        return explicit  # ledger 有值就用 ledger 的

    path = entry.get("path", "")
    module = entry.get("registered_by", "")

    for ptype, pat, auth in _AUTH_HEURISTICS:
        if ptype == "default":
            return auth
        try:
            if ptype == "path":
                if _re.search(pat, path):
                    return auth
            elif ptype == "module":
                if pat in module:
                    return auth
        except _re.error:
            pass
    return None  # 极少见路径不做假设


def axum_to_openapi_path(path: str) -> str:
    """Axum 风格的 `/{foo}` 已经是 OpenAPI 风格 — 不用转。仅做清理。"""
    return path


def normalize_path_for_operation_id(path: str) -> str:
    """去掉版本前缀,生成 operationId-friendly 路径."""
    import re

    cleaned = re.sub(r"/_matrix/client/(?:v\d+|r0)/", "/", path)
    cleaned = re.sub(r"/_matrix/client/unstable/[^/]+/", "/unstable/", cleaned)
    cleaned = cleaned.strip("/")
    cleaned = re.sub(r"[{}]", "", cleaned)
    cleaned = re.sub(r"[^a-zA-Z0-9]+", "_", cleaned)
    return cleaned or "root"


def make_operation_id(method: str, path: str, registered_by: str) -> str:
    """OperationId 形如 `get_login` 或 `post_rooms_roomid_send_event_type_txnid`."""
    m = method.lower()
    p = normalize_path_for_operation_id(path)
    return f"{m}_{p}"


def build_path_params(path: str, path_params: list[str]) -> list[dict[str, Any]]:
    """从 ledger 的 path_params 字段构造 OpenAPI parameter 列表."""
    out: list[dict[str, Any]] = []
    for pname in path_params:
        ptype = PATH_PARAM_TYPES.get(pname, "string")
        out.append({
            "name": pname,
            "in": "path",
            "required": True,
            "description": f"Path parameter {pname!r}",
            "schema": {"type": ptype, "example": f"${{{pname}}}"},
        })
    return out


def build_query_params(query_params: list[str]) -> list[dict[str, Any]]:
    """从 ledger 的 query_params 字段构造 OpenAPI parameter 列表."""
    out: list[dict[str, Any]] = []
    for q in query_params:
        out.append({
            "name": q,
            "in": "query",
            "required": False,
            "description": f"Query parameter {q!r} — TODO: type and description will be filled from handler signatures",
            "schema": {"type": "string"},
        })
    return out


def build_operation(entry: dict[str, Any]) -> dict[str, Any]:
    """从 ledger entry 构造单个 OpenAPI operation."""
    method = entry["method"].lower()
    path = entry["path"]
    op: dict[str, Any] = {
        "summary": f"{entry['method']} {path}",
        "description": (
            f"Auto-generated from RouteLedger.\n"
            f"Source module: `{entry['registered_by']}`.\n"
            f"TODO: Hand-written description, request body schema, and detailed response schema will be added in subsequent iterations."
        ),
        "operationId": make_operation_id(entry["method"], path, entry["registered_by"]),
        "tags": [entry["registered_by"].split("::")[0].split("/")[0] if "/" in entry["registered_by"] else entry["registered_by"]],
        "responses": dict(DEFAULT_RESPONSES),  # 浅拷贝
    }
    # parameters
    params: list[dict[str, Any]] = []
    params.extend(build_path_params(path, entry.get("path_params", [])))
    params.extend(build_query_params(entry.get("query_params", [])))
    if params:
        op["parameters"] = params
    # security (Week 1 Task 2: 启发式推断 + ledger 优先)
    auth = infer_auth(entry)
    if auth and auth in AUTH_TO_SECURITY:
        op["security"] = AUTH_TO_SECURITY[auth]
    # request body placeholder for non-GET methods
    if method in {"post", "put", "patch", "delete"}:
        op["requestBody"] = {
            "required": False,
            "description": "TODO: Per-endpoint request body schema will be filled from handler signatures",
            "content": {"application/json": {"schema": {"type": "object", "additionalProperties": True}}},
        }
    # rate-limit note (B-4)
    if entry.get("rate_limit_exempt"):
        op.setdefault("description", "")
        op["description"] += "\n\n**Rate limit exempt**: This endpoint skips IP-level rate limiting (own per-user+device limit applies)."
    return op


def build_openapi(ledger: dict[str, Any], server_url: str, profile_name: str = "") -> dict[str, Any]:
    """构建完整 OpenAPI 3.0.3 文档.

    Args:
        ledger: 从 ledger.json 解析的 dict
        server_url: server base URL
        profile_name: profile 名 (default/oidc/worker/saml/all),空则用 ledger 自带
    """
    entries = [e for e in ledger["entries"] if "/_matrix/client/" in e["path"]]
    profile_label = profile_name or ledger.get("state_profile", "default")
    profile_flags = ledger.get("profile_flags", {})

    # 按 path 分组 (r0/v3/v1/unstable 都映射到同一 path template)
    by_path: dict[str, dict[str, dict[str, Any]]] = defaultdict(dict)
    for e in entries:
        path = e["path"]
        op = build_operation(e)
        by_path[path][e["method"].lower()] = op

    paths: dict[str, Any] = {}
    for p, methods in sorted(by_path.items()):
        openapi_path = axum_to_openapi_path(p)
        paths[openapi_path] = dict(methods)

    # Auth 推断统计
    auth_counts: dict[str, int] = defaultdict(int)
    for p in paths.values():
        for op in p.values():
            sec = op.get("security", [])
            if not sec:
                auth_counts["optional"] += 1
            elif any("X-Matrix" in s for s in sec):
                auth_counts["federation"] += 1
            elif any("AccessToken" in s for s in sec):
                auth_counts["user_or_admin"] += 1
            else:
                auth_counts["other"] += 1

    desc_lines = [
        "Auto-generated OpenAPI 3 specification for synapse-rust Client-Server API.",
        "",
        f"**Source**: `synapse_ledger_export` binary, schema_version={ledger.get('schema_version', '?')}, "
        f"profile=`{profile_label}`",
        f"**Profile flags**: {profile_flags}",
        f"**Generated at**: {ledger.get('generated_at', '?')}",
        f"**Endpoint count**: {len(entries)} (Client-Server only)",
        f"**Auth coverage (heuristic)**: "
        + ", ".join(f"{k}={v}" for k, v in sorted(auth_counts.items())),
        "",
        "**Coverage**: This spec currently exposes endpoint **metadata** (path, method, auth, path/query params) "
        "auto-derived from the `RouteLedger`. **Request and response schemas are TODOs** — they will be filled in "
        "iteratively:",
        "- Week 1 Task 1: metadata + status codes (default profile)",
        "- Week 1 Task 2: profile-driven splitting (default/oidc/worker/saml/all) + auth heuristic",
        "- Week 2: scan handler signatures for request body schemas",
        "- Week 3: probe live server for response schemas via schemathesis",
        "",
        "Generated by `scripts/api_test/generate_openapi.py`.",
    ]

    doc: dict[str, Any] = {
        "openapi": "3.0.3",
        "info": {
            "title": f"Synapse-Rust Client-Server API (profile: {profile_label})",
            "version": "6.2.0",
            "description": "\n".join(desc_lines),
        },
        "servers": [
            {"url": server_url, "description": "Local development server"},
            {"url": "https://{server_name}", "description": "Federated server (template)", "variables": {"server_name": {"default": "matrix.org", "description": "Target homeserver"}}},
        ],
        "tags": [],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "AccessToken": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "Matrix Access Token",
                    "description": "Standard Matrix `access_token` query param or `Authorization: Bearer <token>` header.",
                },
                "X-Matrix": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "Authorization",
                    "description": "Matrix Federation `X-Matrix` Authorization header (origin-server + key-id + signed-request).",
                },
            },
            "schemas": {
                "MatrixError": MATRIX_ERROR_SCHEMA,
                "GenericResponse": GENERIC_RESPONSE_SCHEMA,
            },
        },
    }
    # 统计 tag
    tag_counts: dict[str, int] = defaultdict(int)
    for p in paths.values():
        for op in p.values():
            for t in op.get("tags", []):
                tag_counts[t] += 1
    doc["tags"] = [{"name": t, "description": f"{c} operation(s) (auto-grouped by ledger source module prefix)"} for t, c in sorted(tag_counts.items(), key=lambda x: -x[1])]
    return doc


# ─────────────────────────────────────────────────────────────────────────────
# Multi-profile mode (Week 1 Task 2)
# ─────────────────────────────────────────────────────────────────────────────
PROFILE_TO_PATH: dict[str, str] = {
    "default": "scripts/api_test/ledger.json",  # current default ledger
    # 其他 profile 需要先跑 export_ledger.sh 重新生成,模板路径如下:
    "oidc": "scripts/api_test/reports/ledger_oidc.json",
    "worker": "scripts/api_test/reports/ledger_worker.json",
    "saml": "scripts/api_test/reports/ledger_saml.json",
    "all": "scripts/api_test/reports/ledger_all.json",
}


def generate_per_profile_specs(
    ledgers: dict[str, dict[str, Any]],
    output_dir: Path,
    server_url: str,
    primary_profile: str = "default",
) -> dict[str, Any]:
    """为每个 profile 生成 spec,返回 manifest dict.

    Args:
        ledgers: {profile_name: ledger_dict}
        output_dir: 输出目录
        server_url: base URL
        primary_profile: 主 spec 名称,会用对应 ledger 生成为 client.yaml

    Returns:
        manifest dict (用于写 index.json)
    """
    output_dir.mkdir(parents=True, exist_ok=True)
    manifest: dict[str, Any] = {
        "schema_version": "1",
        "generated_at": max((l.get("generated_at", "") for l in ledgers.values()), default="?"),
        "primary_profile": primary_profile,
        "specs": {},
    }

    for profile_name, ledger in ledgers.items():
        output_path = output_dir / (f"client-{profile_name}.yaml" if profile_name != primary_profile else "client.yaml")
        doc = build_openapi(ledger, server_url, profile_name=profile_name)
        output_path.write_text(to_yaml(doc), encoding="utf-8")
        cs_count = sum(1 for e in ledger["entries"] if "/_matrix/client/" in e["path"])
        op_count = sum(len(p) for p in doc["paths"].values())
        manifest["specs"][profile_name] = {
            "file": str(output_path.relative_to(output_dir.parent)) if output_path.is_relative_to(output_dir.parent) else str(output_path),
            "ledger_file": PROFILE_TO_PATH.get(profile_name, "?"),
            "profile_flags": ledger.get("profile_flags", {}),
            "ledger_generated_at": ledger.get("generated_at", "?"),
            "client_server_endpoints": cs_count,
            "operations": op_count,
            "tags": len(doc["tags"]),
        }
        print(f"  [openapi] {profile_name:10s} → {output_path.name}  ({op_count} ops, {len(doc['tags'])} tags)")

    return manifest


def _is_yaml_available() -> bool:
    try:
        import yaml  # noqa: F401
        return True
    except ImportError:
        return False


def to_yaml(doc: dict[str, Any]) -> str:
    """把 OpenAPI 字典转 YAML. PyYAML 优先,否则输出 JSON.

    用自定义 Dumper 关闭 alias detection,这样不会因为引用同一个 dict 对象
    而生成 YAML anchors,后续脚本(patch_openapi / scan_handler_schemas /
    probe_responses)可以独立修改每个 operation 而不互相影响.
    """
    if _is_yaml_available():
        import yaml

        class NoAliasDumper(yaml.SafeDumper):
            """禁用 YAML anchor/alias,所有重复对象都展开为完整 value."""
            def ignore_aliases(self, data):
                return True

        return yaml.dump(
            doc,
            Dumper=NoAliasDumper,
            sort_keys=False,
            allow_unicode=True,
            width=120,
            default_flow_style=False,
        )
    else:
        return json.dumps(doc, indent=2, ensure_ascii=False, sort_keys=False) + "\n# NOTE: install PyYAML for YAML output\n"


def main() -> int:
    ap = argparse.ArgumentParser(
        description=(
            "generate_openapi.py — 从 RouteLedger JSON 产物生成 OpenAPI 3.0 规范\n"
            "Usage:\n"
            "  单 profile: python3 generate_openapi.py --ledger ledger.json --output client.yaml\n"
            "  多 profile: python3 generate_openapi.py --all-profiles\n"
            "  显式 profile: python3 generate_openapi.py --profiles default,oidc,worker"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("--ledger", default="scripts/api_test/ledger.json", help="Path to ledger JSON (单 profile 模式)")
    ap.add_argument("--output", default="docs/openapi/client.yaml", help="Output OpenAPI YAML")
    ap.add_argument("--server-url", default="http://localhost:8008", help="Base URL of the synapse-rust server")
    ap.add_argument("--all-profiles", action="store_true", help="生成所有已知 profile 的 spec (default/oidc/worker/saml/all)")
    ap.add_argument(
        "--profiles",
        default="",
        help="逗号分隔的 profile 列表,例如: default,oidc,worker,saml,all (配合 --all-profiles 或单独使用)",
    )
    ap.add_argument("--index", default="docs/openapi/index.json", help="Manifest JSON 输出路径 (--all-profiles 时生效)")
    args = ap.parse_args()

    # ── Multi-profile mode ──────────────────────────────────────────────────────
    if args.all_profiles or args.profiles:
        if args.profiles:
            requested = [p.strip() for p in args.profiles.split(",") if p.strip()]
        else:
            requested = list(PROFILE_TO_PATH.keys())

        output_dir = Path(args.output).parent.resolve()
        ledgers: dict[str, Any] = {}
        missing: list[str] = []

        for profile_name in requested:
            path_str = PROFILE_TO_PATH.get(profile_name)
            if path_str:
                p = Path(path_str)
            else:
                # 用户直接传了 ledger 路径
                p = Path(profile_name)
                profile_name = p.stem.replace("ledger_", "").replace("_", "-")

            if p.exists():
                ledgers[profile_name] = json.loads(p.read_text(encoding="utf-8"))
                print(f"[openapi] loaded ledger {profile_name}: {p}")
            else:
                missing.append(f"  {profile_name:10s} → {p} (不存在,跳过)")

        if missing:
            print("[openapi] 缺少以下 profile ledger (需要先跑 cargo run --bin synapse_ledger_export):")
            for m in missing:
                print(m)
            print("[openapi] 已加载 profile:", list(ledgers.keys()))
            if not ledgers:
                print("ERROR: 没有可用的 ledger,退出", file=sys.stderr)
                return 1

        manifest = generate_per_profile_specs(ledgers, output_dir, args.server_url)
        # 写 manifest
        index_path = Path(args.index)
        index_path.parent.mkdir(parents=True, exist_ok=True)
        index_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"[openapi] manifest: {index_path}")
        return 0

    # ── Single profile mode ───────────────────────────────────────────────────
    ledger_path = Path(args.ledger)
    if not ledger_path.exists():
        print(f"ERROR: ledger file not found: {ledger_path}", file=sys.stderr)
        return 1

    ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    doc = build_openapi(ledger, args.server_url)
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(to_yaml(doc), encoding="utf-8")

    cs_count = sum(1 for e in ledger["entries"] if "/_matrix/client/" in e["path"])
    print(f"[openapi] generated {output_path}")
    print(f"[openapi] Client-Server endpoints: {cs_count}")
    print(f"[openapi] Total operations: {sum(len(p) for p in doc['paths'].values())}")
    print(f"[openapi] Tags: {len(doc['tags'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
