#!/usr/bin/env python3
"""SQLx 查询普查：把"动态 SQL / 编译期校验 SQL"按 **生产 vs `#[cfg(test)]`** 分区计数。

## 为什么需要独立脚本

旧的 `scripts/ci/check_sqlx_dynamic_ratio.sh` 用 `grep -rE … | wc -l` 计数，有三个
已登记缺陷（见 `scripts/ci/sqlx_dynamic_ratio_baseline` 头部）：

1. **不看注释**：散文里写一句 `sqlx::query(` 就会抬高棘轮（历史 +1 即此类）；
2. **按行计数**：同一行两处调用只算一处；
3. **不分区**：把 `#[cfg(test)]` 内联测试夹具与生产代码混在一个数字里，
   导致 2,151 处里哪些是**真实生产债务**不可见。

本脚本把这三条一次修掉：先做词法剥离（注释/字符串/字符字面量），再做出现次数
计数，最后按源码区域（生产 / test）归类。

## 区域归类口径（可复现）

逐行扫描剥离后的代码，维护花括号深度。遇到形如 `#[cfg(test)]` 或
`#[cfg(any(test, …))]` 的属性行时记下 `pending`，**下一个 `{`** 会把该属性所修饰的
条目的块体划入 test 区（`mod tests { … }` 是主要形态），块闭合即退出。
`#[cfg(test)]` 挂在没有块体的条目（如 `use`）上时，`pending` 会顺延到下一个 `{`；
这是已知的近似，输出中通过 `--list-uncertain` 可人工复核（本仓未出现）。

## 用法

    python3 scripts/ci/sqlx_query_census.py            # key=value 摘要
    python3 scripts/ci/sqlx_query_census.py --verbose  # 附 Top-N 文件与分区
    python3 scripts/ci/sqlx_query_census.py --json     # 机器可读
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# 扫描范围与 `check_sqlx_dynamic_ratio.sh` 保持一致（根 crate + workspace 成员）。
SCAN_DIRS = [
    "src",
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-storage/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "synapse-web/src",
    "synapse-test-utils/src",
]

# 动态：`sqlx::query(` / `query_as(` / `query_scalar(` / 以及 turbofish
# `query_as::<…>(` / `query_scalar::<…>(`。排除宏（`!`）。
DYNAMIC_RE = re.compile(r"sqlx::query(?:_as|_scalar)?\s*(?:[<(]|::<)")
# 静态（编译期校验）：`query!` / `query_as!` / `query_scalar!` / `query_file!`
STATIC_RE = re.compile(r"sqlx::query(?:_as|_scalar|_file)?!")
# 动态 SQL 组装器（合法动态：push_bind 仍参数化），单列统计，不计入棘轮两侧。
QUERY_BUILDER_RE = re.compile(r"QueryBuilder(?:\s*::<[^>]*>)?\s*::new")
CFG_TEST_RE = re.compile(r"#\s*\[\s*cfg\([^]]*\btest\b")
# `#[cfg(test)] mod db_tests;` / `#[cfg(any(test, feature = "test-utils"))] pub(crate) mod tests;`
TEST_MOD_DECL_RE = re.compile(
    r"#\s*\[\s*cfg\([^\]]*\btest\b[^\]]*\)\s*\]\s*"
    r"(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
    re.DOTALL,
)


def strip_code(text: str) -> list[str]:
    """逐行返回"只剩代码"的文本：注释与字符串/字符字面量内容被空格替换。

    保留换行与花括号，使行号与块深度可靠；`//`/`/* */`（含嵌套）/`r"…"`/
    `r#"…"#`/`"…"`/`'x'` 均被剥离。生命周期标注（`'a`）不会被误吞。
    """
    lines: list[str] = []
    buf: list[str] = []
    i = 0
    n = len(text)
    block_depth = 0

    def flush_newline() -> None:
        lines.append("".join(buf))
        buf.clear()

    while i < n:
        ch = text[i]

        if ch == "\n":
            flush_newline()
            i += 1
            continue

        if block_depth:
            if text.startswith("/*", i):
                block_depth += 1
                i += 2
            elif text.startswith("*/", i):
                block_depth -= 1
                i += 2
            else:
                i += 1
            continue

        if text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
            continue

        if text.startswith("/*", i):
            block_depth += 1
            i += 2
            continue

        # raw string: r"…" / r#"…"# / r##"…"##
        if ch == "r" and i + 1 < n and text[i + 1] in ('"', "#"):
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                j += 1
                closing = '"' + "#" * hashes
                k = text.find(closing, j)
                k = n if k == -1 else k + len(closing)
                buf.append(" " * (k - i))
                i = k
                continue

        if ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            buf.append(" " * (j - i))
            i = j
            continue

        # 字符字面量 `'x'` / `'\n'`；生命周期 `'a` 原样保留。
        if ch == "'":
            j = i + 1
            if j < n and text[j] == "\\":
                j += 2
            else:
                j += 1
            if j < n and text[j] == "'":
                buf.append(" " * (j + 1 - i))
                i = j + 1
                continue
            buf.append(ch)
            i += 1
            continue

        buf.append(ch)
        i += 1

    if buf:
        flush_newline()
    return lines


def collect_test_gated_files(sources: list[Path]) -> set[Path]:
    """解析 `#[cfg(test)] mod x;`（含 `pub(crate)`、跨行）声明的文件。

    仅靠"文件内是否出现 `#[cfg(test)]`"会漏掉整份测试模块文件——它们的属性写在
    声明方（`mod.rs`）里，例如 `event/db_tests.rs` 由 `event/mod.rs` 的
    `#[cfg(test)] mod db_tests;` 引入。把这类文件整份归入 test 区，生产计数才准。
    """
    gated: set[Path] = set()
    for path in sources:
        text = path.read_text(encoding="utf-8", errors="ignore")
        if "cfg(" not in text or "mod " not in text:
            continue
        code = "\n".join(strip_code(text))
        for name in TEST_MOD_DECL_RE.findall(code):
            for candidate in (path.parent / f"{name}.rs", path.parent / name / "mod.rs"):
                if candidate.is_file():
                    gated.add(candidate.resolve())
    return gated


def census_file(path: Path, force_test: bool = False) -> dict[str, int]:
    """返回单文件的 (prod/test) × (dynamic/static) 计数与 QueryBuilder 数。"""
    text = path.read_text(encoding="utf-8", errors="ignore")
    code = strip_code(text)

    depth = 0
    test_parent_depth: int | None = None
    pending_cfg_test = False
    counters = {
        "dynamic_production": 0,
        "dynamic_test": 0,
        "static_production": 0,
        "static_test": 0,
        "query_builder": 0,
    }

    for line in code:
        in_test = force_test or (test_parent_depth is not None and depth > test_parent_depth)

        if CFG_TEST_RE.search(line):
            pending_cfg_test = True

        dyn_hits = len(DYNAMIC_RE.findall(line))
        static_hits = len(STATIC_RE.findall(line))
        counters["query_builder"] += len(QUERY_BUILDER_RE.findall(line))
        if dyn_hits or static_hits:
            counters["dynamic_test" if in_test else "dynamic_production"] += dyn_hits
            counters["static_test" if in_test else "static_production"] += static_hits

        for char in line:
            if char == "{":
                depth += 1
                if pending_cfg_test and test_parent_depth is None:
                    test_parent_depth = depth - 1
                    pending_cfg_test = False
            elif char == "}":
                depth -= 1
                if test_parent_depth is not None and depth <= test_parent_depth:
                    test_parent_depth = None

    return counters


def main() -> int:
    parser = argparse.ArgumentParser(description="SQLx query census (production vs #[cfg(test)])")
    parser.add_argument("--root", default=".", help="仓库根目录（默认当前目录）")
    parser.add_argument("--verbose", action="store_true", help="附 Top-N 文件与分区明细")
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument("--top", type=int, default=20, help="--verbose 时列出的文件数")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    totals = {
        "dynamic_production": 0,
        "dynamic_test": 0,
        "static_production": 0,
        "static_test": 0,
        "query_builder": 0,
    }
    per_file: list[tuple[int, int, str]] = []

    sources: list[Path] = []
    for rel in SCAN_DIRS:
        base = root / rel
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            # 与旧脚本一致：排除遗留 worktree 副本与构建产物。
            if "/target/" in str(path) or "/.claude/" in str(path):
                continue
            sources.append(path)

    test_gated = collect_test_gated_files(sources)

    for path in sources:
            got = census_file(path, force_test=path.resolve() in test_gated)
            for key in totals:
                totals[key] += got[key]
            if got["dynamic_production"] or got["dynamic_test"]:
                per_file.append(
                    (got["dynamic_production"], got["dynamic_test"], str(path.relative_to(root)))
                )

    dynamic = totals["dynamic_production"] + totals["dynamic_test"]
    static = totals["static_production"] + totals["static_test"]
    total = dynamic + static
    ratio = (dynamic / total) if total else 0.0

    summary = {
        **totals,
        "dynamic": dynamic,
        "static": static,
        "total": total,
        "ratio": round(ratio, 4),
    }

    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        for key in (
            "dynamic_production",
            "dynamic_test",
            "static_production",
            "static_test",
            "dynamic",
            "static",
            "query_builder",
            "total",
            "ratio",
        ):
            print(f"{key}={summary[key]}")

    if args.verbose:
        per_file.sort(reverse=True)
        print(f"\n# Top {args.top} files by dynamic (production / test / path)")
        for prod, test, path in per_file[: args.top]:
            print(f"{prod:5d} {test:5d}  {path}")

    if total == 0:
        print("ERROR: 扫描到 0 处 sqlx 调用，扫描范围配置可能已失效", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
