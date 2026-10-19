#!/usr/bin/env python3
"""F-4.1 — flag "fake success" response bodies made entirely of constants.

Why
---
C-1 是一类真实缺陷：handler 返回 `{"keys": []}` 这种**全常量且带空数组**的
body。HTTP 200 让客户端以为操作成功，实际上服务端什么都没做、也没实现。这类
假成功不会崩溃、不会被类型检查抓到，只会让调用方静默拿到空结果。

判据刻意收得很窄，避免变成"扫到一堆正常代码"的噪音门禁：
  * 位置：`synapse-web/src/routes/**.rs` 里 `Ok(Json(...))` 的返回值；
  * 形状：整个 body 是单个 `json!` 字面量，且**不含任何插值**（没有变量、
    字段访问、函数调用、宏调用）；
  * 内容：含空数组 `[]` 或空对象 `{}`。

只有三条同时成立才报。像 `{"event_id": event.get("event_id"), "keys": []}`
这种"部分动态 + 部分空常量"的写法不在本门禁内（属于 D-2 的 typed 化议题），
否则基线会大到没人看。

豁免必须写理由
--------------
`scripts/contract/suspicious_empty_response_allowlist.txt` 一行一条
`<相对路径>::<fn>\treason=<非空说明>`。门禁是**双向棘轮**：新命中不登记 = 红，
登记过期（被豁免的 fn 不再命中）= 红。理由不是注释：没有 reason 的豁免行
解析即报错，所以"为什么这里可以返回空"必须留下文字，不能靠记忆。

用法
----
  python3 scripts/contract/check_suspicious_empty_response.py
  python3 scripts/contract/check_suspicious_empty_response.py --mutation-check
  python3 scripts/contract/check_suspicious_empty_response.py --report   # 只列命中

退出码
------
  0 = 命中集合与豁免集合一致
  1 = 有新命中 / 豁免过期 / 豁免行格式不合法
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCAN_ROOTS = (ROOT / "synapse-web" / "src" / "routes",)
ALLOWLIST = ROOT / "scripts" / "contract" / "suspicious_empty_response_allowlist.txt"

JSON_MACRO = re.compile(r"(?:serde_json\s*::\s*)?json!\s*\(")
FN_NAME = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
OK_JSON_PREFIX = re.compile(r"Ok\s*\(\s*Json\s*\(\s*$")
OK_JSON_PREFIX_QUALIFIED = re.compile(r"Ok\s*\(\s*Json\s*\(\s*(?:serde_json\s*::\s*)?$")


def strip_comments(src: str) -> str:
    """Blank out `//` and `/* */` comments while preserving offsets/newlines.

    String literals are copied verbatim so a `//` inside them is not mistaken for
    a comment. Raw strings (`r"..."`, `r#"..."#`) are handled because route files
    contain SQL and path literals.
    """
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        ch = src[i]
        if ch == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j == -1 else j
            for k in range(i, j):
                out[k] = " "
            i = j
            continue
        if ch == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            j = n if j == -1 else j + 2
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if ch == "r" and i + 1 < n and src[i + 1] in '#"':
            # Rust raw string: r"..." / r#"..."# / r##"..."##
            m = re.match(r'r(#+)?"', src[i:])
            if m:
                hashes = m.group(1) or ""
                start = i + len(m.group(0))
                end_marker = '"' + hashes
                j = src.find(end_marker, start)
                i = n if j == -1 else j + len(end_marker)
                continue
        if ch == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            i = j
            continue
        i += 1
    return "".join(out)


def find_matching_paren(src: str, open_idx: int) -> int:
    """Index just past the `)` matching the `(` at `open_idx`; -1 if unbalanced."""
    depth = 0
    i, n = open_idx, len(src)
    while i < n:
        ch = src[i]
        if ch == '"':
            i += 1
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    break
                i += 1
            i += 1
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return -1


def without_strings(body: str) -> str:
    """Remove string literals (keys included) and char literals from `body`."""
    out = []
    i, n = 0, len(body)
    while i < n:
        ch = body[i]
        if ch == '"':
            i += 1
            while i < n:
                if body[i] == "\\":
                    i += 2
                    continue
                if body[i] == '"':
                    i += 1
                    break
                i += 1
            out.append(" ")
            continue
        if ch == "'":
            # char literal or lifetime; only skip well-formed `'x'` / `'\n'`
            m = re.match(r"'(?:\\.|[^'\\])'", body[i:])
            if m:
                i += m.end()
                out.append(" ")
                continue
        out.append(ch)
        i += 1
    return "".join(out)


def is_all_constant(body: str) -> bool:
    """True when the literal contains only keys, literal values and punctuation."""
    rest = without_strings(body)
    rest = re.sub(r"[{}\[\],:\s]", "", rest)
    rest = re.sub(r"-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?", "", rest)
    rest = re.sub(r"\b(?:true|false|null)\b", "", rest)
    return rest == ""


def has_empty_container(body: str) -> bool:
    structural = without_strings(body).strip()
    # A wholly empty `{}` is the Matrix convention for "success, no payload"
    # (PUT/DELETE endpoints). It is not a fake success, so strip the outermost
    # braces before looking for nested empties.
    if structural.startswith("{") and structural.endswith("}"):
        structural = structural[1:-1]
    return (
        re.search(r"\[\s*\]", structural) is not None
        or re.search(r"\{\s*\}", structural) is not None
    )


def enclosing_fn(src: str, pos: int) -> str:
    name = "<top-level>"
    for m in FN_NAME.finditer(src, 0, pos):
        name = m.group(1)
    return name


def scan_source(src: str, relpath: str) -> list[dict]:
    """Return findings for one Rust source file (comments are stripped first)."""
    clean = strip_comments(src)
    findings: list[dict] = []
    for m in JSON_MACRO.finditer(clean):
        open_idx = m.end() - 1
        close_idx = find_matching_paren(clean, open_idx)
        if close_idx == -1:
            continue
        # Only responses returned as `Ok(Json(json!(...)))`.
        before = clean[max(0, m.start() - 64) : m.start()]
        if not (
            OK_JSON_PREFIX.search(before) or OK_JSON_PREFIX_QUALIFIED.search(before)
        ):
            continue
        body = clean[open_idx + 1 : close_idx - 1]
        if not (is_all_constant(body) and has_empty_container(body)):
            continue
        line = clean.count("\n", 0, m.start()) + 1
        findings.append(
            {
                "key": f"{relpath}::{enclosing_fn(clean, m.start())}",
                "path": relpath,
                "line": line,
                "body": " ".join(body.split()),
            }
        )
    return findings


def scan_tree() -> list[dict]:
    findings: list[dict] = []
    for root in SCAN_ROOTS:
        for path in sorted(root.rglob("*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            findings.extend(scan_source(path.read_text(encoding="utf-8"), rel))
    return findings


def load_allowlist() -> dict[str, str]:
    """Parse the allowlist; unknown/invalid rows are fatal (no silent typos)."""
    if not ALLOWLIST.exists():
        return {}
    allowed: dict[str, str] = {}
    errors: list[str] = []
    for lineno, raw in enumerate(ALLOWLIST.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 2:
            errors.append(
                f"{ALLOWLIST.name}:{lineno}: expected `<path>::<fn>\\treason=...`"
            )
            continue
        key, meta = parts[0].strip(), parts[1].strip()
        # `reason` is free text and may itself contain `=`, so take everything
        # after the prefix instead of tokenising on whitespace.
        if not meta.startswith("reason=") or not meta[len("reason=") :].strip():
            errors.append(
                f"{ALLOWLIST.name}:{lineno}: only a non-empty `reason=` is allowed"
            )
            continue
        if key in allowed:
            errors.append(f"{ALLOWLIST.name}:{lineno}: duplicate entry for {key}")
            continue
        allowed[key] = meta[len("reason=") :].strip()
    if errors:
        raise SystemExit("\n".join(errors))
    return allowed


def mutation_check() -> None:
    """Both directions must hold: the detector must fire AND stay quiet."""
    positive = """
fn fake(State(ctx): State<Ctx>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "keys": [] })))
}
"""
    negative_dynamic = """
fn real(keys: Vec<Value>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "keys": keys })))
}
"""
    negative_nonempty = """
fn ok(State(ctx): State<Ctx>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "a": 1, "b": true, "c": null })))
}
"""
    negative_no_payload = """
fn deleted(State(ctx): State<Ctx>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({})))
}
"""
    negative_not_a_response = """
fn helper() -> Value {
    let _ = json!({ "keys": [] });
    json!({})
}
"""
    cases = [
        ("constant empty must fire", positive, 1),
        ("dynamic value must not fire", negative_dynamic, 0),
        ("constant non-empty must not fire", negative_nonempty, 0),
        ("spec-style empty `{}` must not fire", negative_no_payload, 0),
        ("non-response literal must not fire", negative_not_a_response, 0),
    ]
    failures = []
    for label, src, expected in cases:
        got = len(scan_source(src, "fixture.rs"))
        if got != expected:
            failures.append(f"  - {label}: expected {expected} finding(s), got {got}")
    if failures:
        raise SystemExit(
            "suspicious-empty-response mutation-check FAILED:\n" + "\n".join(failures)
        )
    print(
        "✅ mutation-check: detector fires on the fake success and stays quiet otherwise."
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mutation-check", action="store_true")
    parser.add_argument(
        "--report", action="store_true", help="list findings without enforcing"
    )
    args = parser.parse_args()

    if args.mutation_check:
        mutation_check()
        return 0

    findings = scan_tree()
    allowed = load_allowlist()
    found = {f["key"]: f for f in findings}

    if args.report:
        for f in findings:
            print(f"{f['key']}\t{f['path']}:{f['line']}\tbody={f['body']}")
        print(f"total: {len(findings)}")
        return 0

    unlisted = sorted(k for k in found if k not in allowed)
    stale = sorted(k for k in allowed if k not in found)

    if not unlisted and not stale:
        print(
            f"✅ suspicious-empty-response: {len(found)} exemption(s), all justified."
        )
        return 0

    lines = ["❌ suspicious-empty-response drift"]
    if unlisted:
        lines.append(f"  unlisted all-constant empty responses ({len(unlisted)}):")
        for key in unlisted:
            f = found[key]
            lines.append(f"    - {key}  ({f['path']}:{f['line']})  body={f['body']}")
        lines.append(
            "  Fix: return real data, or add `<path>::<fn>\\treason=...` to "
            "scripts/contract/suspicious_empty_response_allowlist.txt."
        )
    if stale:
        lines.append(f"  stale exemptions ({len(stale)}):")
        lines += [f"    - {key}" for key in stale]
        lines.append(
            "  Fix: the handler no longer returns a constant empty body — delete the line."
        )
    print("\n".join(lines))
    return 1


if __name__ == "__main__":
    sys.exit(main())
