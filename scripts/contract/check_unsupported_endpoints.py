#!/usr/bin/env python3
"""恒定失败端点必须登记 `unsupported=true` 的双向守卫（RD-15…RD-19）。

背景
----
`§9.7 始终失败但已注册的路由` 的修复方向是「下架或登记」，不是「补实现」。
但「登记」此前只靠人记：`ledger_annotations.txt` 的 `unsupported=true` 行没有
任何守卫对着 handler 源码校验。结果是两侧都可能悄悄漂移：

* 新增一个恒定 `Err(...)` 的 handler（或把既有 handler 改成恒失败）⇒ 契约面
  上看不出它是 by-design 不支持；客户端无法与服务端故障区分。
* 反过来，某条已登记的端点后来真的实现了（handler 出现 `Ok` 分支）⇒
  `unsupported=true` 变成谎言，契约面反而误报。

本脚本把「handler 恒失败」从源码静态判定出来，与登记表**双向**比对：

* **漏登记**：源码判定为恒定失败、但该 `(method, path)` 没有 `unsupported=true` ⇒ 红。
* **陈旧登记**：登记了 `unsupported=true`、但 handler 已不是恒定失败（或路由已不存在）⇒ 红。

判定规则（保守，宁可漏判不可误判）
----------------------------------
一个 handler 被判为「恒定失败」当且仅当同时满足：

1. 函数体（剥离注释 / 字符串 / 字符字面量后）**不出现** `Ok`（含 `Ok(`, `Ok::`,
   `Ok{` 等形态）——一旦有成功分支就不是恒失败；
2. 函数体**最后一个顶层语句**是 `Err(...)` 或 `return Err(...)`。

因此合法的「先校验再恒失败」形态（`validate(x)?; Err(...)`）会被抓到，而
`match` 里分支出 `Ok`、或末尾是 `Err` 但前面有 `Ok` 的 handler 不会被误判。
解析不到的 handler（未解析 / 跨文件同名）直接跳过，由 sidecar 的
`confidence` 与本脚本的 `--self-test` 兜住，不在这里猜。

用法
----
    python3 scripts/contract/check_unsupported_endpoints.py             # 门禁
    python3 scripts/contract/check_unsupported_endpoints.py --self-test # 红/绿自证
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SIDECAR = ROOT / "docs" / "synapse-rust" / "ROUTE_HANDLERS.json"
ROUTES_ROOT = ROOT / "synapse-web" / "src" / "routes"
ANNOTATIONS = ROOT / "scripts" / "contract" / "ledger_annotations.txt"

# `fn name(` — the body is located by matching the parameter parens, then the
# first `{` after them (return types in this codebase never contain braces).
_FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>{}]*>)?\s*\(")


def _sanitize(src: str) -> str:
    """Blank out comments and string/char literals, keeping brace structure.

    Only the structural skeleton matters here (`Ok`/`Err` tokens and statement
    boundaries), so a single state-machine pass is enough — and it must not
    treat a `{` inside a format string as a block.
    """
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            i = n if j < 0 else j + 2
            out.append(" ")
            continue
        if c == "r" and i + 1 < n and src[i + 1] in ('"', "#"):
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                close = '"' + "#" * hashes
                k = src.find(close, j + 1)
                i = n if k < 0 else k + len(close)
                out.append('""')
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            i = j + 1
            out.append('""')
            continue
        if c == "'":
            # A lifetime (`'a`) has no closing quote; a char literal does.
            j = i + 1
            k = -1
            if j < n and src[j] == "\\":
                k = src.find("'", j + 2)
            elif j + 1 < n and src[j + 1] == "'":
                k = j + 1
            if k != -1:
                i = k + 1
                out.append("' '")
                continue
        out.append(c)
        i += 1
    return "".join(out)


def _match_delim(s: str, start: int, open_ch: str, close_ch: str) -> int:
    """Index of the `close_ch` matching `s[start] == open_ch`, or -1."""
    depth = 0
    for i in range(start, len(s)):
        if s[i] == open_ch:
            depth += 1
        elif s[i] == close_ch:
            depth -= 1
            if depth == 0:
                return i
    return -1


def extract_fn_body(src: str, name: str) -> str | None:
    """Body text of the first `fn <name>(...) { ... }` in `src`, sans delimiters."""
    code = _sanitize(src)
    for m in _FN_RE.finditer(code):
        if m.group(1) != name:
            continue
        paren_open = m.end() - 1
        paren_close = _match_delim(code, paren_open, "(", ")")
        if paren_close < 0:
            continue
        brace = code.find("{", paren_close)
        if brace < 0:
            continue
        brace_close = _match_delim(code, brace, "{", "}")
        if brace_close < 0:
            continue
        # Slice the *sanitized* text: offsets no longer line up with `src` once
        # comments and literals have been collapsed.
        return code[brace + 1 : brace_close]
    return None


def _last_top_level_statement(code: str) -> str:
    """Text after the last `;` that sits at brace/paren/bracket depth 0."""
    depth = 0
    last = 0
    for i, c in enumerate(code):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == ";" and depth == 0:
            last = i + 1
    return code[last:].strip()


def classify_body(body: str) -> str | None:
    """`None` if the handler can succeed, else the `ApiError` constructor name.

    See the module docstring for the two-clause rule. The returned string is
    evidence only (used in the failure message), never a verdict input.
    """
    code = _sanitize(body)
    if re.search(r"\bOk\s*(?:\(|<|\{|::)", code):
        return None
    tail = _last_top_level_statement(code)
    if not re.match(r"(?:return\s+)?Err\s*\(", tail):
        return None
    ctor = re.search(r"\bApiError::([a-z_][a-z0-9_]*)", code)
    return ctor.group(1) if ctor else "Err"


def load_declared(path=ANNOTATIONS) -> set:
    """`{(METHOD, path)}` whose row carries `unsupported=true`.

    The annotation file format is pinned by `extract_registered.py`
    (`METHOD PATH<TAB>key=value ...`); this reader only needs the boolean.
    """
    declared = set()
    if not path.exists():
        return declared
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.rstrip("\n")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        address, sep, annotation = line.partition("\t")
        if not sep:
            continue
        parts = address.split()
        if len(parts) != 2:
            continue
        for pair in annotation.split():
            key, _, value = pair.partition("=")
            if key == "unsupported" and value == "true":
                declared.add((parts[0].upper(), parts[1]))
    return declared


def load_handlers(path=SIDECAR) -> dict:
    """`{(METHOD, path): (file, handler)}` from the resolved sidecar."""
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
    routes = {}
    for row in doc["routes"]:
        file = row.get("file")
        handler = row.get("handler")
        if file and handler:
            routes[(row["method"], row["path"])] = (file, handler)
    return routes


def scan(handler_map: dict, routes_root: Path = ROUTES_ROOT) -> dict:
    """`{(METHOD, path): err_ctor}` for every route whose handler is constant-fail."""
    sources: dict = {}
    found = {}
    for key, (rel_file, handler) in sorted(handler_map.items()):
        if rel_file not in sources:
            path = routes_root / rel_file
            sources[rel_file] = (
                path.read_text(encoding="utf-8") if path.exists() else None
            )
        src = sources[rel_file]
        if src is None:
            continue
        body = extract_fn_body(src, handler)
        if body is None:
            continue
        ctor = classify_body(body)
        if ctor:
            found[key] = ctor
    return found


def analyze(found: dict, declared: set, known_routes: set) -> list:
    """Return failure strings (empty == green). Pure, so `--self-test` can drive it."""
    failures = []
    for key in sorted(set(found) - declared):
        failures.append(
            f"{key[0]} {key[1]}: handler always returns Err (ApiError::{found[key]}) "
            f"but no `unsupported=true` annotation — register it as by-design "
            f"unsupported (or implement/remove the route)"
        )
    for key in sorted(declared - set(found)):
        if key not in known_routes:
            failures.append(
                f"{key[0]} {key[1]}: `unsupported=true` declared but the route does "
                f"not exist in ROUTE_HANDLERS.json — stale annotation"
            )
        else:
            failures.append(
                f"{key[0]} {key[1]}: `unsupported=true` declared but the handler is no "
                f"longer a constant failure — drop the annotation"
            )
    return failures


def self_test() -> int:
    checks = []

    def expect(name, got, want):
        checks.append((name, got, want))

    expect(
        "err-only body is constant failure",
        classify_body('Err(ApiError::unrecognized("no bridges"))'),
        "unrecognized",
    )
    expect(
        "validate-then-err body is constant failure",
        classify_body(
            'validate_room_id(&room_id)?;\nlet x = room_id.replace("%21", "!");\n'
            'Err(ApiError::not_supported("nope"))'
        ),
        "not_supported",
    )
    expect(
        "log-then-err body is constant failure",
        classify_body(
            '::tracing::info!(event = "x", origin = ?auth.origin);\n'
            'Err(ApiError::unrecognized("nope"))'
        ),
        "unrecognized",
    )
    expect(
        "body with an Ok branch is not constant failure",
        classify_body('if done { return Ok(Json(v)); }\nErr(ApiError::not_found("x"))'),
        None,
    )
    expect(
        "match arm Ok is not constant failure",
        classify_body(
            'match x { Some(v) => Ok(Json(v)), None => Err(ApiError::not_found("x")) }'
        ),
        None,
    )
    expect(
        "Ok::<T, E> is not constant failure",
        classify_body("Ok::<_, ApiError>(Json(v))"),
        None,
    )
    expect(
        "braces inside a format string do not break the body scan",
        classify_body('Err(ApiError::bad_request("bad {thing}"))'),
        "bad_request",
    )

    # Direction 1: an unregistered constant-failure route must be RED.
    expect(
        "missing annotation is detected",
        bool(analyze({("GET", "/_x"): "unrecognized"}, set(), {("GET", "/_x")})),
        True,
    )
    # Direction 2: a stale annotation must be RED.
    expect(
        "stale annotation is detected",
        bool(analyze({}, {("GET", "/_x")}, {("GET", "/_x")})),
        True,
    )
    # Control: registered + constant-failure is GREEN.
    expect(
        "registered constant failure is green",
        analyze({("GET", "/_x"): "unrecognized"}, {("GET", "/_x")}, {("GET", "/_x")}),
        [],
    )
    # A declared route that no longer exists is RED.
    expect(
        "annotation for a removed route is detected",
        bool(analyze({}, {("GET", "/_gone")}, set())),
        True,
    )

    bad = [c for c in checks if c[1] != c[2]]
    for name, got, want in checks:
        print(f"  {'ok  ' if got == want else 'FAIL'} {name} — got {got!r}")
    if bad:
        print(f"❌ self-test failed for {len(bad)} case(s)")
        return 1
    print(f"✅ self-test: {len(checks)} classifier/guard cases behave as declared")
    return 0


def main(argv) -> int:
    if "--self-test" in argv:
        return self_test()
    handler_map = load_handlers()
    declared = load_declared()
    found = scan(handler_map)
    failures = analyze(found, declared, set(handler_map))
    if failures:
        print("❌ always-failing endpoints are not declared consistently:")
        for f in failures:
            print(f"   - {f}")
        print(
            "\n   Source of truth: scripts/contract/ledger_annotations.txt "
            "(`unsupported=true`). See audit §9.7 RD-15…RD-19."
        )
        return 1
    print(
        f"✅ {len(declared)} registered route(s) declared by-design unsupported; "
        f"{len(found)} constant-failure handler(s) match the declaration"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
