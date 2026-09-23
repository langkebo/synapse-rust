#!/usr/bin/env python3
"""
Week 2 Task 4 — 补 OpenAPI request body schema (主脚本)

策略 (3 阶段):
  Stage A: 扫描路由注册 — 提取 path → handler 函数名
  Stage B: 扫描 handler 函数签名 — 提取 handler → Json<TypeName>
  Stage C: join A+B = path → TypeName 精确映射
  Stage D: 用映射更新 docs/openapi/client.yaml 中的 requestBody
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parents[2]
ROUTES_DIR = ROOT / "synapse-web/src/routes"
OPENAPI_PATH = ROOT / "docs/openapi/client.yaml"
OUTPUT_JSON = ROOT / "scripts/api_test/handler_schemas.json"


# ============ Stage A: 路由注册 → path → handler 名 ============

_ROUTE_RE = re.compile(r'\.route\s*\(\s*"([^"]+)"\s*,\s*([^)]+?)\)', re.DOTALL)
_METHOD_FN_RE = re.compile(r"\b(get|post|put|patch|delete)\s*\(\s*([\w:]+)")


def scan_route_registrations() -> dict[str, list[tuple[str, str]]]:
    mappings: dict[str, list[tuple[str, str]]] = {}
    for f in sorted(ROUTES_DIR.rglob("*.rs")):
        if f.name == "mod.rs":
            continue
        content = f.read_text()
        for m in _ROUTE_RE.finditer(content):
            path = m.group(1)
            handler_block = m.group(2)
            for method, fn in _METHOD_FN_RE.findall(handler_block):
                mappings.setdefault(fn, []).append((path, method))
    return mappings


# ============ Stage B: handler 函数 → Json<TypeName> ============


def scan_handler_signatures() -> dict[str, str]:
    """返回 {handler_name: type_name}.

    策略: 找每个 fn 定义, 收集签名行(从 fn 起到 { 进入函数体前),
    在签名文本中匹配 Json<TypeName>.
    """
    out: dict[str, str] = {}
    fn_re = re.compile(
        r"(?:^|\n)\s*(?:pub(?:\([^)]*\))?\s+)?" r"(?:async\s+)?fn\s+(\w+)\s*\("
    )

    for f in sorted(ROUTES_DIR.rglob("*.rs")):
        if f.name == "mod.rs":
            continue
        lines = f.read_text().split("\n")
        i = 0
        while i < len(lines):
            m = fn_re.search(lines[i])
            if not m:
                i += 1
                continue

            fn_name = m.group(1)
            # 收集从此行到 { 为止(不含函数体)
            param_lines = [lines[i][m.end() :]]
            j = i
            brace_seen = False
            while j < len(lines) and j < i + 50:
                l = lines[j]
                if "{" in l:
                    # 截断到 { 之前
                    idx = l.index("{")
                    if j == i:
                        param_lines[-1] = param_lines[-1] + l[:idx]
                    else:
                        param_lines.append(l[:idx])
                    brace_seen = True
                    break
                j += 1
                if j != i:
                    param_lines.append(l)

            params_text = " ".join(param_lines)
            for tp in re.finditer(
                r"(?:Matrix)?Json\s*\(\s*\w+\s*\)\s*:\s*(?:Matrix)?Json\s*<\s*([A-Z]\w*)\s*>",
                params_text,
            ):
                tn = tp.group(1)
                if tn not in ("Value", "Json", "serde_json"):
                    out.setdefault(fn_name, tn)
            # Advance past this function body (including closing brace)
            i = j + 1 if brace_seen else i + 1
    return out


# ============ Stage B2: Struct 字段 → JSON Schema ============


def extract_struct_fields(content: str, struct_name: str) -> Optional[list[dict]]:
    pattern = re.compile(
        r"#\s*\[\s*derive\s*\(([^)]*\bDeserialize\b[^)]*)\)\s*\][^\n]*\n"
        r"(?:#\s*\[[^\]]*\]\s*\n)*?"
        r"pub\s+struct\s+" + re.escape(struct_name) + r"\s*\{",
        re.MULTILINE,
    )
    m = pattern.search(content)
    if not m:
        return None
    start = m.end()
    depth = 1
    end = start
    for i in range(start, len(content)):
        if content[i] == "{":
            depth += 1
        elif content[i] == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    body = content[start:end]
    fields = []
    pending_attrs = []
    for line in body.split("\n"):
        stripped = line.strip()
        if not stripped or stripped.startswith("//"):
            continue
        if stripped.startswith("#["):
            pending_attrs.append(stripped)
            continue
        if "//" in stripped:
            stripped = stripped[: stripped.index("//")].strip()
        fm = re.match(r"pub\s+(\w+)\s*:\s*(.+?)(?:\s*=\s*[^,;]+)?[,\s;]*$", stripped)
        if not fm:
            pending_attrs = []
            continue
        rust_name = fm.group(1)
        rust_type = fm.group(2).strip().rstrip(",")
        all_attrs = " ".join(pending_attrs + [stripped])
        json_name = rust_name
        has_default = "default" in all_attrs
        is_skipped = re.search(r"\bskip\b", all_attrs) is not None
        optional = rust_type.startswith("Option<") or has_default
        rm = re.search(r'rename\s*=\s*"([^"]+)"', all_attrs)
        if rm:
            json_name = rm.group(1)
        if is_skipped:
            pending_attrs = []
            continue
        fields.append(
            {
                "name": rust_name,
                "json_name": json_name,
                "rust_type": rust_type,
                "optional": optional,
                "has_default": has_default,
            }
        )
        pending_attrs = []
    return fields if fields else None


def rust_type_to_schema(rust_type: str) -> dict:
    rust_type = rust_type.strip()
    opt_match = re.match(r"Option<(.+)>", rust_type)
    if opt_match:
        return {"anyOf": [rust_type_to_schema(opt_match.group(1)), {"type": "null"}]}
    vec_match = re.match(r"Vec<(.+)>", rust_type)
    if vec_match:
        return {"type": "array", "items": rust_type_to_schema(vec_match.group(1))}
    map_match = re.match(r"(?:BTreeMap|HashMap)<.+,\s*(.+)>", rust_type)
    if map_match:
        return {
            "type": "object",
            "additionalProperties": rust_type_to_schema(map_match.group(1)),
        }
    unwrap_match = re.match(r"(?:Box|Arc|Rc)<(.+)>", rust_type)
    if unwrap_match:
        return rust_type_to_schema(unwrap_match.group(1))
    type_map = {
        "String": {"type": "string"},
        "bool": {"type": "boolean"},
        "i8": {"type": "integer", "format": "int8"},
        "i16": {"type": "integer", "format": "int16"},
        "i32": {"type": "integer", "format": "int32"},
        "i64": {"type": "integer", "format": "int64"},
        "u8": {"type": "integer", "format": "uint8"},
        "u16": {"type": "integer", "format": "uint16"},
        "u32": {"type": "integer", "format": "uint32"},
        "u64": {"type": "integer", "format": "uint64"},
        "usize": {"type": "integer"},
        "isize": {"type": "integer"},
        "f32": {"type": "number", "format": "float"},
        "f64": {"type": "number", "format": "double"},
    }
    if rust_type in type_map:
        return type_map[rust_type]
    if rust_type in ("Value", "serde_json::Value", "JsonValue"):
        return {"type": "object", "additionalProperties": True}
    return {
        "type": "object",
        "description": f"rust: {rust_type}",
        "x-unknown-rust-type": rust_type,
    }


def build_object_schema(struct_name: str, fields: list[dict]) -> dict:
    schema = {"type": "object", "properties": {}, "required": []}
    for f in fields:
        schema["properties"][f["json_name"]] = rust_type_to_schema(f["rust_type"])
        if not f["optional"]:
            schema["required"].append(f["json_name"])
    if not schema["required"]:
        del schema["required"]
    return schema


def struct_to_schema(file_path: Path, struct_name: str) -> Optional[dict]:
    content = file_path.read_text()
    fields = extract_struct_fields(content, struct_name)
    if not fields:
        return None
    return build_object_schema(struct_name, fields)


# 全局 struct 索引 — 一次扫描建立 type_name -> file 映射
_STRUCT_DEF_RE = re.compile(
    r"#\s*\[\s*derive\s*\([^)]*\bDeserialize\b[^)]*\)\][^\n]*\n"
    r"(?:#\s*\[[^\]]*\][^\n]*\n)*"
    r"pub\s+struct\s+([A-Z]\w*)\s*\{"
)


def build_struct_index(routes_dir: Path) -> dict[str, Path]:
    """遍历所有 .rs 文件, 找到所有 #[derive(Deserialize)] struct, 返回 type_name -> file 映射."""
    index: dict[str, Path] = {}
    for f in sorted(routes_dir.rglob("*.rs")):
        if f.name == "mod.rs":
            continue
        try:
            content = f.read_text()
        except Exception:
            continue
        for m in _STRUCT_DEF_RE.finditer(content):
            type_name = m.group(1)
            # 重复定义时, 后到者覆盖(同文件名优先)
            index.setdefault(type_name, f)
    return index


# ============ Stage C: join → path → type_name ============


def build_path_type_mapping(
    route_map: dict[str, list[tuple[str, str]]],
    handler_map: dict[str, str],
) -> dict[tuple[str, str], tuple[str, str, str]]:
    """返回 {(method, full_path): (rel_path, method, type_name)}."""
    out: dict[tuple[str, str], tuple[str, str, str]] = {}
    for handler, type_name in handler_map.items():
        routes = route_map.get(handler, [])
        for path, method in routes:
            if method not in ("post", "put", "patch"):
                continue
            # path 可能是相对路径(如 /pushers) 或完整路径
            # 在 OpenAPI spec 中可能有 r0/v3/v1 多个版本
            # 尝试拼成所有可能版本
            full_paths = [path]
            if not path.startswith("/_matrix"):
                full_paths = [
                    "/_matrix/client/r0" + path,
                    "/_matrix/client/v3" + path,
                    "/_matrix/client/v1" + path,
                ]
            for full in full_paths:
                out[(method, full)] = (path, method, type_name)
    return out


# ============ Stage D: patch openapi ============


def patch_openapi(joined_map: dict, type_schemas: dict) -> tuple[dict, int, int, list]:
    import yaml

    spec = yaml.safe_load(OPENAPI_PATH.read_text())

    patched = 0
    unmatched = 0
    matched_pairs = []
    unmatched_samples = []
    for path, path_item in spec.get("paths", {}).items():
        for method in ("post", "put", "patch"):
            operation = path_item.get(method)
            if not operation:
                continue
            key = (method, path)
            if key not in joined_map:
                unmatched += 1
                if len(unmatched_samples) < 5:
                    unmatched_samples.append(
                        (method, path, operation.get("operationId", ""))
                    )
                continue
            rel_path, _, type_name = joined_map[key]
            if type_name not in type_schemas:
                unmatched += 1
                continue
            schema = type_schemas[type_name]["schema"]
            content = operation.setdefault("requestBody", {}).setdefault("content", {})
            content.setdefault("application/json", {})["schema"] = schema
            operation["requestBody"]["required"] = True
            matched_pairs.append(
                {
                    "type": type_name,
                    "op_id": operation.get("operationId", ""),
                    "path": path,
                    "method": method.upper(),
                }
            )
            patched += 1

    spec["x-handler-scan"] = {
        "patched_request_bodies": patched,
        "unmatched_operations": unmatched,
        "types_found": len(type_schemas),
        "matched_pairs": matched_pairs,
        "unmatched_samples": unmatched_samples,
    }

    # Ensure x-handler-scan values are JSON-native (no tuples)
    def _make_serializable(obj):
        if isinstance(obj, tuple):
            return list(obj)
        if isinstance(obj, dict):
            return {k: _make_serializable(v) for k, v in obj.items()}
        if isinstance(obj, list):
            return [_make_serializable(i) for i in obj]
        return obj

    spec["x-handler-scan"] = _make_serializable(spec["x-handler-scan"])
    return spec, patched, unmatched, unmatched_samples


def main() -> int:
    print(f"[scan] ROUTES_DIR = {ROUTES_DIR}")
    print(f"[scan] OPENAPI_PATH = {OPENAPI_PATH}")

    # Stage A
    route_map = scan_route_registrations()
    write_routes = {
        h: [(p, m) for p, m in r if m in ("post", "put", "patch")]
        for h, r in route_map.items()
    }
    write_route_count = sum(len(v) for v in write_routes.values())
    print(
        f"[A] found {len(route_map)} handler registrations, {write_route_count} write routes"
    )

    # Stage B (signatures)
    handler_map = scan_handler_signatures()
    print(f"[B] found {len(handler_map)} handlers with Json<TypeName>")

    # Stage B (struct fields) — 先建立 (type_name -> file) 全局索引
    type_to_file = build_struct_index(ROUTES_DIR)

    type_schemas = {}
    skipped = []
    for type_name, f in type_to_file.items():
        if type_name not in handler_map.values():
            continue
        schema = struct_to_schema(f, type_name)
        if schema:
            type_schemas[type_name] = {
                "schema": schema,
                "file": str(f.relative_to(ROOT)),
                "field_count": len(schema.get("properties", {})),
            }
        else:
            skipped.append({"type_name": type_name, "file": str(f.relative_to(ROOT))})

    print(f"[B] {len(type_schemas)} type schemas, {len(skipped)} skipped")

    # Stage C: join
    joined = build_path_type_mapping(route_map, handler_map)
    print(f"[C] {len(joined)} path→type mappings")

    # 保存扫描结果
    output = {
        "route_registrations": {k: list(v) for k, v in write_routes.items() if v},
        "handler_signatures": handler_map,
        "type_schemas": type_schemas,
        "joined_mapping": {f"{k[0]} {k[1]}": list(v) for k, v in joined.items()},
        "skipped": skipped,
    }
    # Trailing newline: `scripts/quality/format_audit.py` counts a missing final
    # newline as a `.json` drift signal, and `format_check.sh` fails on drift.
    # Regenerating this artifact used to reintroduce the drift it had just cleared.
    OUTPUT_JSON.write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n")
    print(f"[scan] wrote: {OUTPUT_JSON}")

    # Stage D: patch
    spec, patched, unmatched, unmatched_samples = patch_openapi(joined, type_schemas)
    print(f"[D] patched {patched} requestBody schemas; unmatched {unmatched}")
    if unmatched_samples:
        print("[D] unmatched samples:")
        for m, p, op in unmatched_samples:
            print(f"   {m.upper():6s} {p}  op={op}")

    import yaml

    OPENAPI_PATH.write_text(
        yaml.dump(spec, allow_unicode=True, sort_keys=False, default_flow_style=False)
    )
    print(f"[D] wrote: {OPENAPI_PATH}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
