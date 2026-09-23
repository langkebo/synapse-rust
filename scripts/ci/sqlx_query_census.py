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
    python3 scripts/ci/sqlx_query_census.py --list-production-dynamic <root>
        # 逐条列出**生产区**动态调用点：`path:line:literal|runtime`
        # `literal` = SQL 实参是字符串字面量（Phase B2 起禁止新增）
        # `runtime` = 运行期拼装（`&sql` / `&format!(…)`），即已登记的合法残差
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


def strip_code(text: str, pad_comments: bool = False) -> list[str]:
    """逐行返回"只剩代码"的文本：注释与字符串/字符字面量内容被空格替换。

    保留换行与花括号，使行号与块深度可靠；`//`/`/* */`（含嵌套）/`r"…"`/
    `r#"…"#`/`"…"`/`'x'` 均被剥离。生命周期标注（`'a`）不会被误吞。

    `pad_comments=True` 时注释同样以**等长空格**替换（默认 `False` 直接删除）。
    默认行为用于计数（`census_file`）；等长模式让剥离后的文本与原文**逐字符对齐**，
    供 `iter_dynamic_sites` 把词法匹配位置映射回原文以判定 SQL 实参形态。
    """
    lines: list[str] = []
    buf: list[str] = []
    i = 0
    n = len(text)
    block_depth = 0

    def flush_newline() -> None:
        lines.append("".join(buf))
        buf.clear()

    def fill_span(start: int, end: int) -> None:
        """把 `text[start:end]`（字面量内容）替换为等长空格，但**保留换行**。

        换行必须保留：旧实现用 `buf.append(" " * (end - start))`，跨行字符串/
        raw string 里的 `\\n` 被一并吞成空格，`code` 的行数与源文件不再一一对应，
        依赖行号的输出（`--list-production-dynamic` 的 `path:line`）会整段漂移
        （实测同文件可差 30+ 行）。花括号/正则计数不受影响（换行两侧的内容本就
        已被空格替换）。
        """
        for k in range(start, end):
            if text[k] == "\n":
                flush_newline()
            else:
                buf.append(" ")

    while i < n:
        ch = text[i]

        if ch == "\n":
            flush_newline()
            i += 1
            continue

        if block_depth:
            if text.startswith("/*", i):
                block_depth += 1
                if pad_comments:
                    buf.append("  ")
                i += 2
            elif text.startswith("*/", i):
                block_depth -= 1
                if pad_comments:
                    buf.append("  ")
                i += 2
            else:
                if pad_comments:
                    buf.append(" ")
                i += 1
            continue

        if text.startswith("//", i):
            start = i
            while i < n and text[i] != "\n":
                i += 1
            if pad_comments:
                buf.append(" " * (i - start))
            continue

        if text.startswith("/*", i):
            block_depth += 1
            if pad_comments:
                buf.append("  ")
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
                fill_span(i, k)
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
            fill_span(i, j)
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
                fill_span(i, j + 1)
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


def iter_region_lines(code: list[str], force_test: bool = False):
    """逐行产出 `(line, in_test)`；区域判定逻辑的唯一实现。

    `census_file`（计数）与 `iter_dynamic_sites`（逐点定位）共用本生成器，
    避免两处区域口径漂移。产出行内容**先于**该行的花括号深度更新，故与旧
    `census_file` 内联循环逐字等价。
    """
    depth = 0
    test_parent_depth: int | None = None
    pending_cfg_test = False

    for line in code:
        in_test = force_test or (test_parent_depth is not None and depth > test_parent_depth)

        if CFG_TEST_RE.search(line):
            pending_cfg_test = True

        yield line, in_test

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


def census_file(path: Path, force_test: bool = False) -> dict[str, int]:
    """返回单文件的 (prod/test) × (dynamic/static) 计数与 QueryBuilder 数。"""
    text = path.read_text(encoding="utf-8", errors="ignore")
    code = strip_code(text)
    counters = {
        "dynamic_production": 0,
        "dynamic_test": 0,
        "static_production": 0,
        "static_test": 0,
        "query_builder": 0,
    }

    for line, in_test in iter_region_lines(code, force_test):
        dyn_hits = len(DYNAMIC_RE.findall(line))
        static_hits = len(STATIC_RE.findall(line))
        counters["query_builder"] += len(QUERY_BUILDER_RE.findall(line))
        if dyn_hits or static_hits:
            counters["dynamic_test" if in_test else "dynamic_production"] += dyn_hits
            counters["static_test" if in_test else "static_production"] += static_hits

    return counters


def _find_open_paren(stripped: str, start: int) -> int | None:
    """从动态调用匹配起点出发，跳过 turbofish 的 `<…>`，返回调用圆括号的下标。

    只看**尖括号深度为 0** 处的 `(`：`query_as::<_, (i64, String)>(…)` 里的
    元组括号在深度 1，不会误判。
    """
    angle = 0
    i = start
    n = len(stripped)
    while i < n:
        ch = stripped[i]
        if ch == "<":
            angle += 1
        elif ch == ">":
            if angle:
                angle -= 1
        elif ch == "(" and angle == 0:
            return i
        i += 1
    return None


def _first_arg_offset(text: str, start: int) -> int:
    """跳过空白与注释，返回第一个实参 token 在**原文**中的下标。"""
    i = start
    n = len(text)
    while i < n:
        ch = text[i]
        if ch in " \t\r\n":
            i += 1
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
        elif text.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        else:
            return i
    return n


def _starts_string_literal(text: str, i: int) -> bool:
    """`text[i]` 处是否为字符串字面量的起始（含 `r"…"` / `r#"…"#` / `b"…"`）。"""
    if i >= len(text):
        return False
    if text[i] == '"':
        return True
    j = i
    if text.startswith("br", j) or text.startswith("rb", j):
        j += 2
    elif text[j] == "r":
        j += 1
    elif text[j] == "b" and j + 1 < len(text) and text[j + 1] == '"':
        j += 1
    else:
        return False
    while j < len(text) and text[j] == "#":
        j += 1
    return j < len(text) and text[j] == '"'


def iter_dynamic_sites(path: Path, force_test: bool = False) -> list[tuple[int, str, str]]:
    """列出单文件全部动态 sqlx 调用点：`(行号, 区域, 实参形态)`。

    * 区域：`"production"` / `"test"`，口径与 `census_file` 同源；
    * 实参形态：`"literal"` = SQL 实参是字符串**字面量**；`"runtime"` = 其它
      表达式（`&sql` / `&query` / `&format!(…)`），即 Phase B2 允许的残差类别。

    实参形态靠**等长剥离**（`strip_code(..., pad_comments=True)`）把词法匹配位置
    映射回原文后判定：`(pad_comments=False)` 会删除注释、使列偏移错位。
    """
    text = path.read_text(encoding="utf-8", errors="ignore")
    code = strip_code(text, pad_comments=True)
    stripped = "\n".join(code)

    offsets: list[int] = []
    pos = 0
    for line in code:
        offsets.append(pos)
        pos += len(line) + 1

    sites: list[tuple[int, str, str]] = []
    for li, (line, in_test) in enumerate(iter_region_lines(code, force_test)):
        for match in DYNAMIC_RE.finditer(line):
            kind = "runtime"
            open_paren = _find_open_paren(stripped, offsets[li] + match.start())
            if open_paren is not None:
                arg = _first_arg_offset(text, open_paren + 1)
                if _starts_string_literal(text, arg):
                    kind = "literal"
            sites.append((li + 1, "test" if in_test else "production", kind))
    return sites


def _is_excluded(root: Path, path: Path) -> bool:
    """排除构建产物与遗留 worktree 副本（**相对扫描根**判定，不与绝对路径耦合）。

    旧实现用 `"/target/" in str(path)`：当仓库本身位于一级 `target/` 目录之下
    （例如 `git worktree add target/cd-wt` 的验证树）时，**所有**源文件都含该子串，
    扫描面被整体清空 —— 守卫会以"0 个站点"假通过。改为按相对根的路径分量判定后，
    主树行为不变（`SCAN_DIRS` 从不落在 `target/`/`.claude/` 内），验证树也能被正常扫描。
    """
    try:
        parts = path.relative_to(root).parts
    except ValueError:
        return True
    return any(part in ("target", ".claude") for part in parts)


def collect_sources(root: Path) -> list[Path]:
    """按 `SCAN_DIRS` 收集待扫 `.rs`（排除遗留 worktree 副本与构建产物）。"""
    sources: list[Path] = []
    for rel in SCAN_DIRS:
        base = root / rel
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            if _is_excluded(root, path):
                continue
            sources.append(path)
    return sources


def list_production_dynamic(root: Path) -> int:
    """打印生产区每个动态调用点 `path:line:literal|runtime`（供守卫测试消费）。"""
    sources = collect_sources(root)
    test_gated = collect_test_gated_files(sources)
    for path in sources:
        for line_no, region, kind in iter_dynamic_sites(path, force_test=path.resolve() in test_gated):
            if region != "production":
                continue
            print(f"{path.relative_to(root).as_posix()}:{line_no}:{kind}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="SQLx query census (production vs #[cfg(test)])")
    parser.add_argument("--root", default=".", help="仓库根目录（默认当前目录）")
    parser.add_argument("--verbose", action="store_true", help="附 Top-N 文件与分区明细")
    parser.add_argument("--json", action="store_true", help="输出 JSON")
    parser.add_argument("--top", type=int, default=20, help="--verbose 时列出的文件数")
    parser.add_argument(
        "--list-production-dynamic",
        nargs="?",
        const="",
        default=None,
        metavar="ROOT",
        help="列出生产区每个动态调用点 path:line:literal|runtime（ROOT 缺省取 --root）",
    )
    args = parser.parse_args()

    if args.list_production_dynamic is not None:
        return list_production_dynamic(Path(args.list_production_dynamic or args.root).resolve())

    root = Path(args.root).resolve()
    totals = {
        "dynamic_production": 0,
        "dynamic_test": 0,
        "static_production": 0,
        "static_test": 0,
        "query_builder": 0,
    }
    per_file: list[tuple[int, int, str]] = []

    sources = collect_sources(root)

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
