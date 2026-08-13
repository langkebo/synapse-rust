#!/usr/bin/env python3
"""#18 泛化 DB 错误批量替换脚本（变体 A 治理工具）。

识别存储层「手动 tracing::error!/warn! + ApiError::database("A database error occurred")」
三段式模式（含多行与单行两种写法），并分类为：

  * 变体 A/A'（可直接替换）：tracing 消息含具体操作语义（"Failed to ..."、
    "Invalid auth data" 等），操作名可直接复用。
  * 变体 B（需人工补全）：tracing 消息为纯 "Database error: {e}"，无操作语义，
    脚本按最近的方法名给出建议，需人工确认。

默认只生成报告（dry-run），加 `--apply` 才对变体 A/A' 执行替换（变体 B 永不自动改）。

用法：
    python3 scripts/replace_generic_db_errors.py            # 报告到 stdout
    python3 scripts/replace_generic_db_errors.py --apply    # 执行变体 A/A' 替换
    python3 scripts/replace_generic_db_errors.py --tsv out.tsv
"""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOTS = ["synapse-e2ee/src", "src", "synapse-services/src", "tests"]

SKIP_FILES = {
    "synapse-e2ee/src/megolm/storage.rs",  # 阶段 1 试点已完成
    "synapse-common/src/error.rs",  # From<sqlx::Error> 兜底，有意保留
}

DB_GENERIC_RE = re.compile(r'ApiError::database\("A database error occurred"')
TRACING_RE = re.compile(r'tracing::(?:error|warn)!\("([^"]*)"')
# map_err 可出现在行首、也可在方法链中间（`.xxx(...).map_err(|e| {`）
MAP_ERR_RE = re.compile(r'\.map_err\(\|[a-zA-Z_][a-zA-Z0-9_]*\|\s*\{')
FN_RE = re.compile(r'(?:pub\s+)?(?:async\s+)?fn\s+(\w+)')
GENERIC_MSGS = {"Database error: {e}", "Database error: {}", "Database error", "database error"}


@dataclass
class Finding:
    path: str
    db_line: int
    map_err_line: int | None  # None = 找不到 map_err（应人工）
    single_line: bool
    tracing_msg: str | None
    fn_name: str | None
    kind: str = ""
    operation: str = ""

    @property
    def replaceable(self) -> bool:
        return self.kind == "A" and self.map_err_line is not None


def _extract_operation(msg: str) -> str:
    for suffix in (": {e}", " {e}", ": {}", " {}"):
        if msg.endswith(suffix):
            return msg[: -len(suffix)]
    return msg


def scan_file(path: Path) -> list[Finding]:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError):
        return []

    findings: list[Finding] = []
    fn_at_line: dict[int, str] = {}
    for i, ln in enumerate(lines, start=1):
        m = FN_RE.search(ln)
        if m:
            fn_at_line[i] = m.group(1)

    for i, ln in enumerate(lines, start=1):
        if not DB_GENERIC_RE.search(ln):
            continue

        tracing_msg: str | None = None
        map_err_line: int | None = None
        single_line = False

        # 1) 单行变体：`.map_err(|e| { tracing::error!(...); ApiError::database(...) })?;`
        m = TRACING_RE.search(ln)
        if m and ".map_err" in ln:
            tracing_msg = m.group(1)
            map_err_line = i
            single_line = True

        # 2) 多行变体：向上回溯找 map_err 行与 tracing 行
        if tracing_msg is None or map_err_line is None:
            for j in range(i - 1, max(0, i - 16), -1):
                prev = lines[j - 1]
                if tracing_msg is None:
                    mm = TRACING_RE.search(prev)
                    if mm:
                        tracing_msg = mm.group(1)
                if map_err_line is None and MAP_ERR_RE.search(prev):
                    map_err_line = j
                if tracing_msg is not None and map_err_line is not None:
                    break

        fn_name = None
        for j in range(i - 1, 0, -1):
            if j in fn_at_line:
                fn_name = fn_at_line[j]
                break

        f = Finding(path=str(path), db_line=i, map_err_line=map_err_line, single_line=single_line,
                    tracing_msg=tracing_msg, fn_name=fn_name)
        if tracing_msg is None:
            f.kind = "?"
        elif tracing_msg in GENERIC_MSGS:
            f.kind = "B"
            f.operation = fn_name or "<unknown_fn>"
        else:
            f.kind = "A"
            f.operation = _extract_operation(tracing_msg)
        findings.append(f)
    return findings


def load_findings() -> list[Finding]:
    repo_root = Path(__file__).resolve().parent.parent
    out: list[Finding] = []
    for root in ROOTS:
        base = repo_root / root
        if not base.exists():
            continue
        for p in base.rglob("*.rs"):
            rel = p.relative_to(repo_root).as_posix()
            if rel in SKIP_FILES:
                continue
            out.extend(scan_file(p))
    return out


def report(findings: list[Finding]) -> str:
    a = [f for f in findings if f.kind == "A"]
    b = [f for f in findings if f.kind == "B"]
    unknown = [f for f in findings if f.kind == "?"]
    out: list[str] = [
        f"#18 泛化 DB 错误摸底报告：共 {len(findings)} 处",
        f"  变体 A/A'（可直接替换）: {len(a)} 处",
        f"  变体 B（需方法名回填）: {len(b)} 处",
        f"  无 tracing（需人工）  : {len(unknown)} 处",
        "",
        "=" * 100,
        "【变体 A/A' — 可直接替换】操作名 = tracing 消息去掉 ': {e}' 后缀",
        "=" * 100,
    ]
    for f in sorted(a, key=lambda x: (x.path, x.db_line)):
        out.append(f"{f.path}:{f.db_line}\t-> .map_err(map_database!(\"{f.operation}\"))")
    out += ["", "=" * 100, "【变体 B — 需人工补全】建议操作名取自方法名，替换前需人工确认语义", "=" * 100]
    for f in sorted(b, key=lambda x: (x.path, x.db_line)):
        out.append(f"{f.path}:{f.db_line}\t方法 {f.fn_name} -> .map_err(map_database!(\"{f.operation}\"))")
    if unknown:
        out += ["", "=" * 100, "【无 tracing 前缀 — 需人工核查】", "=" * 100]
        for f in sorted(unknown, key=lambda x: (x.path, x.db_line)):
            out.append(f"{f.path}:{f.db_line}\t（无 tracing::error!/warn!，可能是注释或跨行消息）")
    return "\n".join(out)


def _ensure_import(lines: list[str]) -> list[str]:
    """确保文件已导入 `use synapse_common::map_database;`（宏）。"""
    if any("map_database" in ln and "use synapse_common" in ln for ln in lines):
        return lines
    # 优先插入到第一个 `use synapse_common::` 行之前，保持同 crate import 聚集
    for idx, ln in enumerate(lines):
        if ln.startswith("use synapse_common::"):
            lines.insert(idx, "use synapse_common::map_database;")
            return lines
    # 否则插入到第一个 `use ` 行之后
    for idx, ln in enumerate(lines):
        if ln.startswith("use "):
            lines.insert(idx + 1, "use synapse_common::map_database;")
            return lines
    lines.insert(0, "use synapse_common::map_database;")
    return lines


def _apply_file(rel: str, fs: list[Finding], repo_root: Path) -> int:
    p = repo_root / rel
    lines = p.read_text(encoding="utf-8").splitlines()
    applied = 0
    for f in sorted(fs, key=lambda x: x.db_line, reverse=True):
        db_i = f.db_line - 1
        map_i = f.map_err_line - 1

        if f.single_line and map_i == db_i:
            # 单行：`.map_err(|e| { tracing...; ApiError... })?;` → 整行替换
            raw = lines[db_i]
            idx = raw.find(".map_err")
            prefix = raw[:idx] if idx >= 0 else ""
            stripped = raw.rstrip()
            suffix = ";"
            if stripped.endswith(","):
                suffix = ","
            elif not stripped.endswith(";"):
                suffix = ""
            lines[db_i] = f'{prefix}.map_err(map_database!("{f.operation}"))?{suffix}'
        else:
            # 多行：把 map_err 行的 `.map_err(|X| {` 换成 `.map_err(map_database!("op"))`，
            # 删除中间的 tracing/database 行与结尾 `})...` 行，结尾符号补到 map_err 行。
            map_line = lines[map_i]
            new_map = re.sub(
                r'\.map_err\(\|[a-zA-Z_][a-zA-Z0-9_]*\|\s*\{\s*$',
                f'.map_err(map_database!("{f.operation}"))',
                map_line,
            )
            # 结尾符号：db_i+1 行形如 `})?;` / `})?,` / `})?` / `})`
            suffix = ";"
            has_q = True
            if db_i + 1 < len(lines):
                close = lines[db_i + 1].rstrip()
                m = re.search(r'\}\)(\?)?([;,])?', close)
                if m:
                    has_q = m.group(1) is not None
                    suffix = m.group(2) or ""
            new_map += f'?{suffix}' if has_q else suffix
            del lines[map_i + 1 : db_i + 2]
            lines[map_i] = new_map
        applied += 1

    lines = _ensure_import(lines)
    p.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return applied


def apply_a(findings: list[Finding]) -> dict[str, int]:
    by_file: dict[str, list[Finding]] = {}
    for f in findings:
        if f.replaceable:
            by_file.setdefault(f.path, []).append(f)
    repo_root = Path(__file__).resolve().parent.parent
    changed: dict[str, int] = {}
    for rel, fs in by_file.items():
        n = _apply_file(rel, fs, repo_root)
        if n:
            changed[rel] = n
    return changed


def main() -> int:
    ap = argparse.ArgumentParser(description="#18 泛化 DB 错误治理脚本")
    ap.add_argument("--apply", action="store_true", help="对变体 A/A' 执行替换（变体 B 永不自动改）")
    ap.add_argument("--tsv", metavar="FILE", help="额外输出 TSV 清单到指定文件")
    args = ap.parse_args()

    findings = load_findings()
    print(report(findings))

    if args.tsv:
        with open(args.tsv, "w", encoding="utf-8") as fh:
            fh.write("kind\tpath\tline\tfn\toperation\n")
            for f in sorted(findings, key=lambda x: (x.kind, x.path, x.db_line)):
                fh.write(f"{f.kind}\t{f.path}\t{f.db_line}\t{f.fn_name or ''}\t{f.operation}\n")

    if args.apply:
        changed = apply_a(findings)
        total = sum(changed.values())
        print(f"\n[apply] 已替换 {total} 处（{len(changed)} 个文件）：")
        for rel, n in sorted(changed.items()):
            print(f"  {rel}: {n} 处")
        a_missing = [f for f in findings if f.kind == "A" and f.map_err_line is None]
        if a_missing:
            print(f"[apply] 注意：{len(a_missing)} 处变体 A 因未定位到 map_err 未改，需人工。")
        print("[apply] 变体 B 与无 tracing 处未自动改，需按报告人工补全。")
    else:
        print("\n[dry-run] 仅生成报告，未修改任何文件。加 --apply 执行变体 A/A' 替换。")

    return 0


if __name__ == "__main__":
    sys.exit(main())
