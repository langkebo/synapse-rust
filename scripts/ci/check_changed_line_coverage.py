#!/usr/bin/env python3
"""变更行的覆盖率（PR 车道的门禁）：只问"这次改动新增的行，有没有被跑到"。

## 为什么不是"和 `coverage_baseline.json` 比"

PR 车道为了压成本只跑**受影响 crate 自己的 lib 测试**（`cargo llvm-cov -p <crate>`），
这个读数是**下界**，与合并基线不可比。实测（本机，2026-10-28）：

| 文件 | 合并基线 | 单腿（根 crate lib） |
|---|---|---|
| `src/bin/healthcheck.rs` | 10.91% | **0.00%** |
| `src/bin/synapse_worker.rs` | 38.19% | **0.00%** |
| `src/server/mod.rs` | 32.30% | **22.63%** |

差异全部来自"这一步没跑那些测试"，不是"代码退化了"。拿下界去比对合并基线，
几乎每个被改到的文件都会红 —— 那种门禁两周内必然被静音（§3.1 的 188 文件红同理）。
所以本工具**只读 lcov，不读基线**，判据换成"变更行的命中比例"。

## 判据

1. 变更行 = `+++ b/<path>` 对应的 hunk 里**新增**的行（`-` 侧的行不算：删除与上下文
   都不能由"有没有被跑到"来判定）；只看 `.rs`。
2. 覆盖率 = 已命中的变更行 / 可执行变更行（不在 lcov `DA:` 里的行 —— 注释、空行、
   纯括号 —— 不计入分母，与 llvm-cov 的口径一致）。
3. 变更文件**必须出现在报告里**：缺失 ⇒ exit 2（`NODATA`）。这是"守卫的守卫" ——
   `affected_crates.py` 少算一个 crate、或那条腿编译失败时，绝不能让本步静默通过。
4. 总比例低于 `--min-percent`（默认 **0**，见下）⇒ exit 1。

## `--min-percent` 的默认值为什么是 0

单腿读数是下界：PR 在 `synapse-web` 加的 route handler 可能只被根 crate 的集成
测试覆盖，而 PR 车道并不跑集成测试 ⇒ 按 50% 判红会在这种 PR 上**常态假红**。
所以默认只报数不判红（`0`），把"升级为门禁"留成需要数据的决定：先累积样本，看清
"新增行的单腿命中率"分布，再把阈值 0 → 30 → 50 逐级抬上去。真正带牙齿的是判据 1–3。

## 用法

    python3 scripts/ci/check_changed_line_coverage.py --report coverage/lcov.info --base origin/main
    python3 scripts/ci/check_changed_line_coverage.py --report x.info --diff-file fixture.patch
    python3 scripts/ci/check_changed_line_coverage.py --mutation-check

退出码：0 = 达标；1 = 低于 `--min-percent`；2 = 变更文件不在报告里 / 输入不可用。
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

# 复用同一份 lcov 解析（单一真源）；插入 sys.path 之后导入是有意的。
from merge_lcov import parse_lcov

HUNK = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")
PLUS_HEADER = re.compile(r"^\+\+\+ b/(.+)$")

# PR 车道只跑 `--lib`（lib 测试）。这些目录里的文件属于**别的 target**
# （集成测试 / 基准 / 示例），lib 腿里根本没有它们 ⇒ 既不能算"未覆盖"，也不能算 NODATA。
NON_LIB_SEGMENTS = ("tests/", "benches/", "examples/")


def is_non_lib_target(path: str) -> bool:
    """`tests/…`、`synapse-x/tests/…` 等不属于 lib target 的路径。"""
    return any(f"/{segment}" in f"/{path}" for segment in NON_LIB_SEGMENTS)


def parse_patch(text: str) -> dict[str, set[int]]:
    """unified diff → `{仓库相对路径: {新增行号}}`（只看 `.rs`）。

    新增行直接由 hunk 头的 `+<start>,<count>` 决定：它比"逐行数 `+` 前缀"稳 ——
    后者会把行内容本身以 `++` 开头的代码（注释框、`++i`）和 `+++` 文件头搅在一起。
    `---` 之后的 `+++` 才是新文件（`+++ /dev/null` 是删除，不匹配正则 ⇒ 跳过）。
    """
    added: dict[str, set[int]] = {}
    path: str | None = None
    for raw in text.splitlines():
        if raw.startswith("--- "):
            path = None
            continue
        if raw.startswith("+++ "):
            header = PLUS_HEADER.match(raw)
            candidate = header.group(1).strip() if header else ""
            path = candidate if candidate.endswith(".rs") else None
            if path is not None:
                added.setdefault(path, set())
            continue
        hunk = HUNK.match(raw)
        if hunk is None or path is None:
            continue
        start = int(hunk.group(1))
        count = int(hunk.group(2) or "1")
        added[path].update(range(start, start + count))
    return {file: lines for file, lines in added.items() if lines}


def git_patch(base: str, head: str | None) -> str:
    spec = f"{base}...{head}" if head else f"{base}...HEAD"
    result = subprocess.run(
        [
            "git",
            "-c",
            "core.quotePath=false",
            "diff",
            "--unified=0",
            spec,
            "--",
            "*.rs",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(f"git diff failed: {result.stderr.strip()[:400]}")
    return result.stdout


def index_records(report: str) -> dict[str, dict[int, int]]:
    """`{lcov 里的绝对路径: {行号: 命中次数}}`。"""
    return {sf: da for sf, (_block, da) in parse_lcov(report).items()}


def lookup(path: str, records: dict[str, dict[int, int]]) -> dict[int, int] | None:
    """仓库相对路径 → lcov 记录。llvm-cov 写的是绝对路径，按后缀匹配。"""
    if path in records:
        return records[path]
    suffix = "/" + path
    for key, da in records.items():
        if key.endswith(suffix):
            return da
    return None


def measure(
    added: dict[str, set[int]],
    records: dict[str, dict[int, int]],
) -> tuple[list[dict], list[str], list[str]]:
    """返回 `(rows, missing, non_lib)`。

    `rows` 只含**有可执行变更行**的文件；`missing` 是"有变更行但报告里没有"的生产源码
    （fail-closed）；`non_lib` 是集成测试/基准/示例 —— 它们不在 lib 腿里，属预期缺失。
    """
    rows: list[dict] = []
    missing: list[str] = []
    non_lib: list[str] = []
    for path in sorted(added):
        if is_non_lib_target(path):
            non_lib.append(path)
            continue
        da = lookup(path, records)
        if da is None:
            missing.append(path)
            continue
        executable = sorted(line for line in added[path] if line in da)
        if not executable:
            continue
        hit = [line for line in executable if da[line] > 0]
        rows.append(
            {
                "path": path,
                "executable": len(executable),
                "hit": len(hit),
                "pct": 100.0 * len(hit) / len(executable),
                "missed": [line for line in executable if da[line] <= 0][:10],
            }
        )
    return rows, missing, non_lib


def verdict(
    rows: list[dict], missing: list[str], min_percent: float
) -> tuple[int, str]:
    if missing:
        return 2, f"{len(missing)} 个变更文件不在覆盖率报告里（NODATA）"
    if not rows:
        return 0, "没有可执行的变更行（纯注释/纯删除/非 .rs）"
    total = sum(row["executable"] for row in rows)
    hits = sum(row["hit"] for row in rows)
    pct = 100.0 * hits / total
    if pct + 1e-9 < min_percent:
        return (
            1,
            f"变更行覆盖率 {pct:.1f}% < 阈值 {min_percent:g}%（{hits}/{total} 行）",
        )
    return 0, f"变更行覆盖率 {pct:.1f}%（{hits}/{total} 行），阈值 {min_percent:g}%"


def render(
    rows: list[dict],
    missing: list[str],
    non_lib: list[str],
    summary: str,
    min_percent: float,
) -> None:
    print("=== 变更行覆盖率（**下界**：只跑到受影响 crate 自己的 lib 测试） ===")
    if rows:
        width = max(len(row["path"]) for row in rows)
        for row in sorted(rows, key=lambda r: (r["pct"], r["path"])):
            missed = f"  未命中行 {row['missed']}" if row["missed"] else ""
            print(
                f"  {row['path'].ljust(width)}  {row['hit']:4d}/{row['executable']:4d} 行  "
                f"{row['pct']:6.1f}%{missed}"
            )
    if non_lib:
        print(
            f"  跳过 {len(non_lib)} 个非 lib target 的变更文件（集成测试/基准/示例，lib 腿不测）"
        )
    if missing:
        print("  ⚠️ 以下变更文件不在报告里（该 crate 没被测到 / 编译失败）：")
        for path in missing:
            print(f"     - {path}")
    print(f"  → {summary}")
    if min_percent <= 0:
        print(
            "  ℹ️ 阈值 0 = 只报数不判红：单腿读数是下界，按固定阈值判红会在"
            "『改动由集成测试覆盖』的 PR 上常态假红。"
        )


def mutation_check() -> int:
    """四组合成输入各自的预期结论 —— 阈值/分母/缺失判定任一处坏掉都会被抓到。"""
    records = {"/work/repo/src/probe.rs": {1: 1, 2: 0, 3: 3, 10: 0}}
    cases: list[tuple[str, dict[str, set[int]], float, int]] = [
        ("all changed lines hit, threshold 50", {"src/probe.rs": {1, 3}}, 50.0, 0),
        ("no changed line executed, threshold 50", {"src/probe.rs": {2, 10}}, 50.0, 1),
        ("a changed file missing from the report", {"src/absent.rs": {1}}, 0.0, 2),
        ("only non-executable lines changed", {"src/probe.rs": {7, 8}}, 90.0, 0),
    ]
    failures = 0
    for label, added, threshold, expected in cases:
        rows, missing, _non_lib = measure(added, records)
        code, summary = verdict(rows, missing, threshold)
        status = "OK  " if code == expected else "MISS"
        if code != expected:
            failures += 1
        print(f"{status} {label} -> exit {code} (expected {expected}): {summary}")
    if failures:
        print(
            f"mutation-check FAILED: {failures} case(s) landed on the wrong verdict",
            file=sys.stderr,
        )
        return 1
    print("mutation-check OK: every synthetic case lands on its expected verdict")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--report", default="coverage/lcov.info")
    parser.add_argument("--base", default=None, help="对比基线（如 origin/main）")
    parser.add_argument("--head", default=None)
    parser.add_argument(
        "--diff-file", default=None, help="用现成的 unified diff 代替 git diff"
    )
    parser.add_argument("--min-percent", type=float, default=0.0)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--mutation-check", action="store_true")
    args = parser.parse_args()

    if args.mutation_check:
        return mutation_check()

    report = Path(args.report)
    if not report.is_file():
        print(f"❌ 覆盖率报告不存在：{report}（采集步骤没写出 lcov）", file=sys.stderr)
        return 2

    if args.diff_file:
        text = Path(args.diff_file).read_text()
    elif args.base:
        text = git_patch(args.base, args.head)
    else:
        print("❌ 需要 --base（对比基线）或 --diff-file（现成 patch）", file=sys.stderr)
        return 2

    added = parse_patch(text)
    if not added:
        print("变更行覆盖率：本次改动没有新增任何 .rs 行（纯删除/纯重命名/非代码改动）")
        return 0

    records = index_records(str(report))
    rows, missing, non_lib = measure(added, records)
    code, summary = verdict(rows, missing, args.min_percent)

    if args.json:
        print(
            json.dumps(
                {
                    "rows": rows,
                    "missing": missing,
                    "non_lib": non_lib,
                    "exit": code,
                    "summary": summary,
                    "min_percent": args.min_percent,
                },
                ensure_ascii=False,
                indent=2,
            )
        )
    else:
        render(rows, missing, non_lib, summary, args.min_percent)
        if code == 2:
            print(
                "  ⇒ fail-closed：PR 车道必须**真的测到**改动的文件，"
                "否则『没测』会看起来像『通过』。",
                file=sys.stderr,
            )
    return code


if __name__ == "__main__":
    sys.exit(main())
