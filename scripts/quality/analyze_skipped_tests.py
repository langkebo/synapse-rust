#!/usr/bin/env python3
"""把 `#[ignore]` 跳过用例分成「设计内跳过」与「覆盖缺口」，并校验覆盖车道存在。

为什么需要它：
  `cargo nextest` 只报告 `16 skipped` 这一个数字，无法区分：
    - 设计内跳过：E2E 需要真实 homeserver、性能冒烟按墙钟断言、bench 包装会嵌套
      cargo build lock、生成器只在权限/加密升级时按需执行；
    - 覆盖缺口：新加的 `#[ignore]` 没有任何车道执行，用例永久静默。
  本脚本把每条 `#[ignore]` 归类，并要求该类的覆盖车道真实存在；任何落在四类
  之外的 `#[ignore]`、缺少 reason 的属性、或车道文件缺失，都会让门禁非零退出。

  历史背景：此处曾有一个只读 skipped.txt 的分析器，调用点写了 `|| true`，
  既产不出报告也不会失败。现在调用点不带 `|| true`，分析器也不依赖构建产物。

用法：
  python3 scripts/quality/analyze_skipped_tests.py [--repo-root PATH]

退出码：
  0 = 全部分类完成且覆盖车道齐全
  1 = 存在覆盖缺口 / 缺少 reason / 覆盖车道缺失
  2 = 用法错误
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

# 只扫描真实源码目录；排除构建产物与并行 worktree，避免把别的分支的用例算进来。
EXCLUDED_DIRS = {".git", ".worktrees", "target", "node_modules", "artifacts"}

IGNORE_RE = re.compile(r'^\s*#\[ignore(?:\s*=\s*"((?:[^"\\]|\\.)*)")?\s*\]\s*$')
FN_RE = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)")

CATEGORIES = ("e2e", "perf_manual", "bench_wrapper", "generator")

LANE_LABELS = {
    "e2e": ".github/workflows/e2e-http.yml（定时 + dispatch，--run-ignored only）",
    "perf_manual": ".github/workflows/benchmark.yml（run_performance_manual，--ignored）",
    "bench_wrapper": "scripts/check_pagination_benchmark.py + BENCH_REQUIRE=pagination",
    "generator": "手工按需执行（reason 已写明入口）",
}

# 每类必须真实存在的车道标记：(文件, 必需子串)。子串为 None 时只要求文件存在。
LANE_CHECKS: dict[str, list[tuple[str, str | None]]] = {
    "e2e": [
        (".github/workflows/e2e-http.yml", "ci_backend_validation.sh e2e"),
        ("scripts/ci_backend_validation.sh", "--run-ignored only"),
    ],
    "perf_manual": [
        (".github/workflows/benchmark.yml", "performance_manual"),
        (".github/workflows/benchmark.yml", "--ignored"),
    ],
    "bench_wrapper": [
        ("scripts/check_pagination_benchmark.py", None),
        (".github/workflows/benchmark.yml", "BENCH_REQUIRE"),
    ],
    "generator": [],
}


@dataclass(frozen=True)
class SkipCase:
    file: str
    name: str
    reason: str
    category: str | None


def classify(rel_path: str, reason: str) -> str | None:
    """按「目录/目标 + reason 关键词」归类；对不上任何规则即视为覆盖缺口。"""
    if rel_path.startswith("tests/e2e/"):
        return "e2e" if "E2E_RUN=1" in reason else None
    if rel_path.startswith("tests/performance/"):
        return "perf_manual" if "手工负载冒烟" in reason else None
    if rel_path == "tests/unit/pagination_gate_tests.rs":
        return "bench_wrapper" if "cargo bench" in reason else None
    if re.match(r"^synapse-(web|e2ee)/src/", rel_path) and (
        "generator:" in reason or "golden file" in reason
    ):
        return "generator"
    return None


def scan(repo_root: Path) -> list[SkipCase]:
    cases: list[SkipCase] = []
    for base in sorted(repo_root.iterdir()):
        if not base.is_dir() or base.name in EXCLUDED_DIRS:
            continue
        for path in sorted(base.rglob("*.rs")):
            rel = path.relative_to(repo_root).as_posix()
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            for index, line in enumerate(lines):
                match = IGNORE_RE.match(line)
                if not match:
                    # 形如 `#[ignore` 却无法解析（多行属性/拼写异常）也要报缺口，
                    # 否则会从报告里静默消失。
                    if line.strip().startswith("#[ignore"):
                        cases.append(
                            SkipCase(
                                rel, "<unparsed>", "<无法解析的 ignore 属性>", None
                            )
                        )
                    continue
                reason = match.group(1) or ""
                name = ""
                for lookahead in lines[index + 1 : index + 8]:
                    fn_match = FN_RE.match(lookahead)
                    if fn_match:
                        name = fn_match.group(1)
                        break
                if not reason.strip():
                    category = None
                else:
                    category = classify(rel, reason)
                cases.append(SkipCase(rel, name or "<unknown>", reason, category))
    return cases


def check_lanes(repo_root: Path) -> list[str]:
    problems: list[str] = []
    for category, checks in LANE_CHECKS.items():
        for rel_file, needle in checks:
            target = repo_root / rel_file
            if not target.is_file():
                problems.append(f"{category}: 覆盖车道文件缺失 {rel_file}")
                continue
            if needle is not None and needle not in target.read_text(
                encoding="utf-8", errors="replace"
            ):
                problems.append(f"{category}: {rel_file} 中找不到车道标记 `{needle}`")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(
        description="跳过用例分类报告（设计内跳过 / 覆盖缺口）"
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="仓库根目录，默认取脚本上两级",
    )
    args = parser.parse_args()

    repo_root: Path = args.repo_root.resolve()
    if not (repo_root / "Cargo.toml").is_file():
        print(f"[skip-report] 不是仓库根目录: {repo_root}", file=sys.stderr)
        return 2

    cases = scan(repo_root)
    gaps = [case for case in cases if case.category is None]

    counts = {category: 0 for category in CATEGORIES}
    for case in cases:
        if case.category is not None:
            counts[case.category] += 1

    print("[skip-report] 跳过分类报告（design_intended / coverage_gap）")
    print(f"  {'类别':<14}{'数量':>4}  覆盖车道")
    for category in CATEGORIES:
        print(f"  {category:<14}{counts[category]:>4}  {LANE_LABELS[category]}")
    print(f"  {'coverage_gap':<14}{len(gaps):>4}")
    print(
        f"  合计 {len(cases)} 条：design_intended={len(cases) - len(gaps)}"
        f"，coverage_gap={len(gaps)}"
    )

    lane_problems = check_lanes(repo_root)

    if gaps:
        print(
            "\n[skip-report] 以下 #[ignore] 不属于任何设计内类别（覆盖缺口）：",
            file=sys.stderr,
        )
        for case in gaps:
            reason = case.reason.strip() or "<缺少 reason>"
            print(f"  - {case.file}::{case.name}  reason={reason!r}", file=sys.stderr)
    for problem in lane_problems:
        print(f"[skip-report] {problem}", file=sys.stderr)

    if gaps or lane_problems:
        print(
            "[skip-report] FAIL：跳过用例未归类或覆盖车道缺失。"
            "请为用例补覆盖车道，或把类别/车道规则加入本脚本。",
            file=sys.stderr,
        )
        return 1

    print("[skip-report] OK：全部跳过用例均为设计内跳过，且覆盖车道存在。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
