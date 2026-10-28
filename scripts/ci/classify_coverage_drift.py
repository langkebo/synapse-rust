#!/usr/bin/env python3
"""把 per-file 覆盖率的「基线 vs 本次」差异**按成因分类**，而不是按数值排。

## 为什么需要它

CI 的 `Per-file coverage ratchet (B.3)` 只报一行 `[TOUCHED] <path>: 19.4% < 74.6%`
之类的短落，人工无法回答那个真正要判的问题：**这是代码退步，还是这次根本没测它？**

2026-10-27 的实测给出了答案的一半：`run_coverage.sh` 的 storage 腿原本不带
`friends,widgets,burn-after-read`，于是 `friend_room/repository.rs`(1390 行) /
`widget.rs`(1332) / `burn_after_read.rs`(1066) **根本不参与编译**，报告里永远是 0.0%，
基线里那三条 0.0 就是这么冻住的。加上这三个 feature 之后，读数从「74.59% → 19.4%」
这类跳变，成因**不是代码退化**，而是"同一批文件这次终于被测了"。

所以差异必须先分类再看数值：

  * `GONE-SOURCE`      —— 基线条目对应的源文件已不在磁盘上（删除/重命名）。**非回归**。
  * `GONE-NOT-BUILT`   —— 源文件在，但本次报告里没有它。该文件这次没被编译进测试
                          （典型成因：feature 门控 / target 未选中）。**不是代码退化**。
  * `ZEROED`           —— 源文件在、报告里有它，但覆盖率是 0.0% 而基线 > 0。
                          几乎必然是"这次没有测试执行到它"（同上）。**不是代码退化**。
  * `DROP`             —— 有实测值且低于基线。**这才是要人看的**：新增了未覆盖代码，
                          或删掉了覆盖它的测试。
  * `RISE` / `EQUAL`   —— 正常收紧。
  * `NEW-FILE`         —— 基线里没有的新文件，受 ramp-up / core 门槛约束。

本脚本**只报告，不判红**（判红仍由 `check_file_coverage.py` 负责）。它存在的理由是：
棘轮的落红摘要无法承载"为什么"，而"为什么"决定该修代码还是该修口径。

## 用法

    python3 scripts/ci/classify_coverage_drift.py \
        --report coverage/lcov.info \
        --baseline scripts/ci/coverage_baseline.json \
        --core-files scripts/ci/core_file_coverage_prefixes.txt

加 `--markdown` 输出可直接贴进审计文档的表格；`--only-drops` 只看要看的部分。
"""

from __future__ import annotations

import argparse
import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent


def _load_ratchet():
    """复用棘轮自身的解析/归一化实现（铁律 2：口径只能有一份）。"""
    path = ROOT / "scripts" / "check_file_coverage.py"
    spec = importlib.util.spec_from_file_location("check_file_coverage", path)
    if spec is None or spec.loader is None:  # pragma: no cover - 结构错误
        raise SystemExit(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _matches(rel: str, prefixes: list[str]) -> bool:
    return any(rel.startswith(p) for p in prefixes)


def classify(
    current: dict[str, float],
    baseline: dict[str, float],
    core_prefixes: list[str],
    existing_sources: set,
) -> list[tuple[str, str, float, float]]:
    """Return `(kind, path, baseline_pct, current_pct)` for every path either side knows."""
    rows: list[tuple[str, str, float, float]] = []
    for path in sorted(set(current) | set(baseline)):
        cur = current.get(path)
        prev = baseline.get(path)

        if prev is None and cur is not None:
            rows.append(("NEW-FILE", path, -1.0, cur))
            continue

        if cur is None:
            on_disk = path in existing_sources
            kind = "GONE-NOT-BUILT" if on_disk else "GONE-SOURCE"
            rows.append((kind, path, prev, -1.0))
            continue

        if cur == 0.0 and prev > 0.0:
            rows.append(("ZEROED", path, prev, cur))
        elif cur < prev:
            rows.append(("DROP", path, prev, cur))
        elif cur > prev:
            rows.append(("RISE", path, prev, cur))
        else:
            rows.append(("EQUAL", path, prev, cur))
    return rows


# 输出顺序：越"要人看"的越前。GONE-NOT-BUILT / ZEROED 排在 DROP 之前，
# 因为它们数量大且**不是回归** —— 先看它们才不会把噪音当缺陷去追。
KIND_ORDER = [
    "DROP",
    "GONE-NOT-BUILT",
    "ZEROED",
    "GONE-SOURCE",
    "NEW-FILE",
    "RISE",
    "EQUAL",
]

KIND_NOTE = {
    "DROP": "真实下降：新增未覆盖代码，或覆盖它的测试被删/被跳过",
    "GONE-NOT-BUILT": "源文件在，但本次报告里没有它 ⇒ 这次没被编译进测试（feature/target 口径）",
    "ZEROED": "有记录但实测 0.0% ⇒ 这次没有测试执行到它（同上），不是代码退化",
    "GONE-SOURCE": "源文件已不在磁盘上（删除/重命名）⇒ 非回归",
    "NEW-FILE": "基线里没有的新文件，受 ramp-up / core 门槛约束",
    "RISE": "上升：棘轮会随之收紧",
    "EQUAL": "与基线一致",
}


def fmt(pct: float) -> str:
    return "—" if pct < 0 else f"{pct:.1f}%"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Classify per-file coverage drift by cause."
    )
    parser.add_argument("--report", required=True, type=pathlib.Path)
    parser.add_argument("--baseline", required=True, type=pathlib.Path)
    parser.add_argument("--core-files", type=pathlib.Path, default=None)
    parser.add_argument("--markdown", action="store_true", help="emit a markdown table")
    parser.add_argument(
        "--only-drops",
        action="store_true",
        help="only print DROP rows (the ones that are actually regressions)",
    )
    args = parser.parse_args()

    ratchet = _load_ratchet()
    if not args.report.exists():
        print(f"coverage report not found: {args.report}", file=sys.stderr)
        return 1
    if not args.baseline.exists():
        print(f"baseline not found: {args.baseline}", file=sys.stderr)
        return 1

    current = ratchet.parse_lcov(args.report)
    baseline = ratchet.load_baseline(args.baseline)
    core_prefixes = ratchet.load_prefix_list(args.core_files)
    existing = set(ratchet._normalized_source_paths())

    rows = classify(current, baseline, core_prefixes, existing)

    counts: dict[str, int] = {}
    for kind, _, _, _ in rows:
        counts[kind] = counts.get(kind, 0) + 1

    print(
        f"coverage report : {args.report} ({len(current)} file(s))\n"
        f"baseline        : {args.baseline} ({len(baseline)} entr(y/ies))\n"
        f"source on disk  : {len(existing)} file(s) considered\n"
    )
    print("=== 按成因汇总 ===")
    for kind in KIND_ORDER:
        if kind in counts:
            print(f"  {kind:<16} {counts[kind]:>4}  {KIND_NOTE[kind]}")
    print()

    if args.only_drops:
        shown = [r for r in rows if r[0] == "DROP"]
    else:
        shown = [r for r in rows if r[0] != "EQUAL"]

    if not shown:
        print("nothing to show.")
        return 0

    order = {k: i for i, k in enumerate(KIND_ORDER)}
    shown.sort(key=lambda r: (order.get(r[0], 99), r[3] if r[3] >= 0 else 1e9, r[1]))

    if args.markdown:
        print("| 类别 | 文件 | 基线 | 本次 | core |")
        print("|---|---|---|---|---|")
        for kind, path, prev, cur in shown:
            core = "core" if _matches(path, core_prefixes) else ""
            print(f"| `{kind}` | `{path}` | {fmt(prev)} | {fmt(cur)} | {core} |")
    else:
        for kind, path, prev, cur in shown:
            core = " [core]" if _matches(path, core_prefixes) else ""
            print(
                f"{kind:<16} {path}{core}\n{'':<16} baseline {fmt(prev)} -> now {fmt(cur)}"
            )

    drops = [r for r in rows if r[0] == "DROP"]
    core_drops = [r for r in drops if _matches(r[1], core_prefixes)]
    print(
        f"\n结论：{len(drops)} 条真实下降（其中 {len(core_drops)} 条落在 core-path），"
        f"{counts.get('GONE-NOT-BUILT', 0) + counts.get('ZEROED', 0)} 条是「这次没测它」而不是退化。"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
