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
  * `DENOM`            —— 低于基线，但**分母（LF）变大、命中行（LH）没减少**。新增的
                          代码没被覆盖，绝对覆盖量没退步。基线里同时记了 `lines_found`
                          才判得出来，否则落回 `DROP`。
  * `LOST-HITS`        —— 低于基线，且**命中行绝对减少**（超过 1 行）。真回归：覆盖它
                          的测试被删/被跳过，或执行路径改道。同样要求基线记了 `lines_found`。
  * `DROP`             —— 低于基线且**无法归因**（基线没记 `lines_found`，或变化不足 1 行）。
                          **这才是要人看的**，但它不等于"真回归"：2026-10-28 那 128 条
                          里混着"新增未覆盖代码"，只是老基线没有分母可算。
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


def _attribute_drop(
    prev_pct: float,
    cur_pct: float,
    prev_lf: int | None,
    cur_lf: int | None,
) -> str:
    """把一次"低于基线"归因成 `DENOM` / `LOST-HITS` / `DROP`（未知）。

    两边 `LF` 都已知才算得动：`LH ≈ LF × pct / 100`（pct 存两位小数，误差远小于 1 行）。
    三种结局：

    * 分母变大且命中行没减少（容差 1 行）⇒ `DENOM`：新代码没被覆盖，**不是退化**；
    * 命中行减少超过 1 行 ⇒ `LOST-HITS`：**真退化**（测试被删/跳过，或路径改道）；
    * 其余（分母增长同时命中行也掉了、或变化在 1 行以内、或 LF 未知）⇒ `DROP`。

    容差只留 1 行而不是按比例放宽：`pct` 的两位小数在 1000 行文件上的误差是 0.05 行，
    真正的分界是"整行" —— 放宽到"±5% 就算没变"会把真退化也吞掉。
    """
    if not prev_lf or not cur_lf:
        return "DROP"
    prev_hits = prev_lf * prev_pct / 100.0
    cur_hits = cur_lf * cur_pct / 100.0
    if cur_lf > prev_lf and cur_hits + 1.0 >= prev_hits:
        return "DENOM"
    if cur_hits + 1.0 < prev_hits:
        return "LOST-HITS"
    return "DROP"


def classify(
    current: dict[str, float],
    baseline: dict[str, float],
    core_prefixes: list[str],
    existing_sources: set,
    is_test_only,
    current_lf: dict[str, int] | None = None,
    baseline_lf: dict[str, int] | None = None,
) -> list[tuple[str, str, float, float, int | None, int | None]]:
    """Return `(kind, path, baseline_pct, current_pct)` for every path either side knows.

    `is_test_only` and the `round(cur, 2)` below exist so this tool reaches the **same
    verdict** as the ratchet it explains. Skipping either one makes the tool report
    DROPs the ratchet would not: unmeasurable test-support files, and every file whose
    ratio cannot be written exactly in two decimals (`2/3`, `1/3`, `1/7` …) — the
    ratchet rounds `cur` to storage precision for exactly that reason (see
    `check_file_coverage.py`, "与基线**同一精度**比较").
    """
    rows: list[tuple[str, str, float, float, int | None, int | None]] = []
    current_lf = current_lf or {}
    baseline_lf = baseline_lf or {}
    for path in sorted(set(current) | set(baseline)):
        cur = current.get(path)
        prev = baseline.get(path)
        prev_lf = baseline_lf.get(path)
        cur_lf = current_lf.get(path)

        if is_test_only(path):
            continue

        if prev is None and cur is not None:
            rows.append(("NEW-FILE", path, -1.0, round(cur, 2), None, cur_lf))
            continue

        if cur is None:
            on_disk = path in existing_sources
            kind = "GONE-NOT-BUILT" if on_disk else "GONE-SOURCE"
            rows.append((kind, path, prev, -1.0, prev_lf, None))
            continue

        cur = round(cur, 2)
        if cur == 0.0 and prev > 0.0:
            rows.append(("ZEROED", path, prev, cur, prev_lf, cur_lf))
        elif cur < prev:
            kind = _attribute_drop(prev, cur, prev_lf, cur_lf)
            rows.append((kind, path, prev, cur, prev_lf, cur_lf))
        elif cur > prev:
            rows.append(("RISE", path, prev, cur, prev_lf, cur_lf))
        else:
            rows.append(("EQUAL", path, prev, cur, prev_lf, cur_lf))
    return rows


# 输出顺序：越"要人看"的越前。GONE-NOT-BUILT / ZEROED 排在 DROP 之前，
# 因为它们数量大且**不是回归** —— 先看它们才不会把噪音当缺陷去追。
KIND_ORDER = [
    "LOST-HITS",
    "DROP",
    "DENOM",
    "GONE-NOT-BUILT",
    "ZEROED",
    "GONE-SOURCE",
    "NEW-FILE",
    "RISE",
    "EQUAL",
]

# 所有"低于基线"的类别：`--only-drops` 与结论行都按这个集合数，否则新加的两类会
# 从"要人看的"里蒸发（那正是这个脚本存在的理由）。
BELOW_BASELINE = ("LOST-HITS", "DROP", "DENOM")

KIND_NOTE = {
    "LOST-HITS": "命中行**绝对减少**（>1 行）⇒ 真回归：覆盖它的测试被删/跳过，或路径改道",
    "DENOM": "分母（LF）变大、命中行未减 ⇒ 新增代码没被覆盖，绝对覆盖量没退步",
    "DROP": "低于基线但无法归因（基线未记 lines_found，或变化不足 1 行）⇒ 需要人看",
    "GONE-NOT-BUILT": "源文件在，但本次报告里没有它 ⇒ 这次没被编译进测试（feature/target 口径）",
    "ZEROED": "有记录但实测 0.0% ⇒ 这次没有测试执行到它（同上），不是代码退化",
    "GONE-SOURCE": "源文件已不在磁盘上（删除/重命名）⇒ 非回归",
    "NEW-FILE": "基线里没有的新文件，受 ramp-up / core 门槛约束",
    "RISE": "上升：棘轮会随之收紧",
    "EQUAL": "与基线一致",
}


def fmt_lf(prev_lf: int | None, cur_lf: int | None) -> str:
    """`769→769` / `?→45`：未知一侧用 `?`，绝不填 0。

    填 0 会让 "老的不知道" 与 "新的是 0 行" 变成同一个字符串，而这个脚本的全部价值
    就在于把这两种处境分开。
    """
    return f"{prev_lf if prev_lf else '?'}→{cur_lf if cur_lf else '?'}"


def fmt(pct: float) -> str:
    # 两位小数：基线与实测都按 `round(v, 2)` 比较，只显示一位会把
    # "1.25% → 1.30%" 印成 "1.3% → 1.3%"，读者第一反应是工具坏了。
    return "—" if pct < 0 else f"{pct:.2f}%"


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
        help=(
            "only print rows below the baseline (LOST-HITS / DROP / DENOM); "
            "注意 DROP 是「无法归因」，不等于回归"
        ),
    )
    args = parser.parse_args()

    ratchet = _load_ratchet()
    if not args.report.exists():
        print(f"coverage report not found: {args.report}", file=sys.stderr)
        return 1
    if not args.baseline.exists():
        print(f"baseline not found: {args.baseline}", file=sys.stderr)
        return 1

    current_lf: dict[str, int] = {}
    current = ratchet.parse_lcov(args.report, current_lf)
    baseline_lf: dict[str, int] = {}
    baseline = ratchet.load_baseline(args.baseline, baseline_lf)
    core_prefixes = ratchet.load_prefix_list(args.core_files)
    existing = set(ratchet._normalized_source_paths())

    rows = classify(
        current,
        baseline,
        core_prefixes,
        existing,
        ratchet.is_test_only,
        current_lf=current_lf,
        baseline_lf=baseline_lf,
    )

    counts: dict[str, int] = {}
    for kind, *_rest in rows:
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
        shown = [r for r in rows if r[0] in BELOW_BASELINE]
    else:
        shown = [r for r in rows if r[0] != "EQUAL"]

    if not shown:
        print("nothing to show.")
        return 0

    order = {k: i for i, k in enumerate(KIND_ORDER)}
    shown.sort(key=lambda r: (order.get(r[0], 99), r[3] if r[3] >= 0 else 1e9, r[1]))

    if args.markdown:
        print("| 类别 | 文件 | 基线 | 本次 | LF | core |")
        print("|---|---|---|---|---|---|")
        for kind, path, prev, cur, prev_lf, cur_lf in shown:
            core = "core" if _matches(path, core_prefixes) else ""
            print(
                f"| `{kind}` | `{path}` | {fmt(prev)} | {fmt(cur)}"
                f" | {fmt_lf(prev_lf, cur_lf)} | {core} |"
            )
    else:
        for kind, path, prev, cur, prev_lf, cur_lf in shown:
            core = " [core]" if _matches(path, core_prefixes) else ""
            print(
                f"{kind:<16} {path}{core}\n"
                f"{'':<16} baseline {fmt(prev)} -> now {fmt(cur)}   LF {fmt_lf(prev_lf, cur_lf)}"
            )

    below = [r for r in rows if r[0] in BELOW_BASELINE]
    core_below = [r for r in below if _matches(r[1], core_prefixes)]
    print(
        f"\n结论：{len(below)} 条低于基线（其中 {len(core_below)} 条落在 core-path）："
        f"LOST-HITS {counts.get('LOST-HITS', 0)} = 真回归，"
        f"DENOM {counts.get('DENOM', 0)} = 分母增长（非退化），"
        f"DROP {counts.get('DROP', 0)} = 无法归因（基线没记 lines_found）。"
        f"另有 {counts.get('GONE-NOT-BUILT', 0) + counts.get('ZEROED', 0)} 条是"
        f"「这次没测它」而不是退化。"
    )
    unknown = counts.get("DROP", 0)
    if unknown:
        print(
            "  ⚠️ DROP 不等于回归：`lines_found` 是从 2026-10-28 起才记进基线的，老条目"
            "在'低于 floor'那一支上分母永远补不上（见 `save_baseline` 的成对规则）。"
            "要归因只能看源文件在基线冻结后有没有长大 —— 用例见 "
            "`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1.7。"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
