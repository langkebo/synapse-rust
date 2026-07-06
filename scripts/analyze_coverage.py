#!/usr/bin/env python3
"""Analyze lcov.info and emit a Markdown coverage report.

Usage:
    python3 scripts/analyze_coverage.py
    python3 scripts/analyze_coverage.py --threshold 80
    python3 scripts/analyze_coverage.py --crate synapse-storage
    python3 scripts/analyze_coverage.py --threshold 80 --crate synapse-services

Input:
    coverage/lcov.info (lcov format)

Output:
    Markdown report to stdout.

Classification thresholds (per file):
    ✅ 达标     coverage >= threshold (default 80%)
    ⚠️ 警告     warn_floor <= coverage < threshold   (default warn_floor=70%)
    ❌ 不达标   0 < coverage < warn_floor
    ⛔ 零覆盖   LH == 0 or no coverage data
"""

from __future__ import annotations

import argparse
import pathlib
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Dict, List, Optional

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_LCOV = ROOT / "coverage" / "lcov.info"

# Crate detection: maps path prefix -> crate label
CRATE_PREFIXES = [
    ("synapse-common/src/", "synapse-common"),
    ("synapse-cache/src/", "synapse-cache"),
    ("synapse-storage/src/", "synapse-storage"),
    ("synapse-e2ee/src/", "synapse-e2ee"),
    ("synapse-federation/src/", "synapse-federation"),
    ("synapse-services/src/", "synapse-services"),
    ("src/", "main (src/)"),
]


@dataclass
class FileCoverage:
    rel_path: str
    crate: str
    lf: int  # total lines
    lh: int  # hit lines
    pct: float
    status: str  # "PASS" | "WARN" | "FAIL" | "ZERO"

    @property
    def status_icon(self) -> str:
        return {
            "PASS": "✅",
            "WARN": "⚠️",
            "FAIL": "❌",
            "ZERO": "⛔",
        }[self.status]


@dataclass
class CrateStats:
    crate: str
    files: List[FileCoverage] = field(default_factory=list)
    total_lf: int = 0
    total_lh: int = 0

    @property
    def avg_pct(self) -> float:
        # weighted by lines: total_lh / total_lf
        if self.total_lf == 0:
            return 0.0
        return (self.total_lh / self.total_lf) * 100.0

    @property
    def mean_pct(self) -> float:
        # unweighted mean across files
        if not self.files:
            return 0.0
        return sum(f.pct for f in self.files) / len(self.files)

    @property
    def failing_files(self) -> List[FileCoverage]:
        return [f for f in self.files if f.status in ("FAIL", "ZERO")]


def detect_crate(rel_path: str) -> str:
    for prefix, crate in CRATE_PREFIXES:
        if rel_path.startswith(prefix):
            return crate
    return "other"


def classify(pct: float, lh: int, threshold: float, warn_floor: float) -> str:
    if lh == 0:
        return "ZERO"
    if pct >= threshold:
        return "PASS"
    if pct >= warn_floor:
        return "WARN"
    return "FAIL"


def parse_lcov(path: pathlib.Path, threshold: float, warn_floor: float) -> List[FileCoverage]:
    """Parse lcov.info and return list of FileCoverage records.

    Only files under src/ directories (any crate) with .rs extension are kept.
    """
    if not path.exists():
        raise FileNotFoundError(f"lcov.info not found: {path}")

    results: List[FileCoverage] = []
    current_path: Optional[str] = None
    current_lf: int = 0
    current_lh: int = 0

    def flush() -> None:
        nonlocal current_path, current_lf, current_lh
        if current_path is None:
            return
        rel = _to_rel(current_path)
        if rel is None or not rel.endswith(".rs"):
            current_path = None
            current_lf = 0
            current_lh = 0
            return
        if "/src/" not in "/" + rel and not rel.startswith("src/"):
            current_path = None
            current_lf = 0
            current_lh = 0
            return
        crate = detect_crate(rel)
        lf = current_lf
        lh = current_lh
        pct = (lh / lf * 100.0) if lf > 0 else 0.0
        status = classify(pct, lh, threshold, warn_floor)
        results.append(
            FileCoverage(
                rel_path=rel,
                crate=crate,
                lf=lf,
                lh=lh,
                pct=round(pct, 2),
                status=status,
            )
        )
        current_path = None
        current_lf = 0
        current_lh = 0

    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("SF:"):
                current_path = line[3:]
            elif line.startswith("LF:"):
                try:
                    current_lf = int(line[3:])
                except ValueError:
                    current_lf = 0
            elif line.startswith("LH:"):
                try:
                    current_lh = int(line[3:])
                except ValueError:
                    current_lh = 0
            elif line == "end_of_record" or line == "end_of_record:":
                flush()
    flush()
    return results


def _to_rel(abs_path: str) -> Optional[str]:
    """Convert an absolute SF: path to a repo-relative path."""
    try:
        rel = str(pathlib.Path(abs_path).relative_to(ROOT))
        return rel.replace("\\", "/")
    except ValueError:
        return abs_path.replace("\\", "/")


def render_markdown(
    files: List[FileCoverage],
    threshold: float,
    warn_floor: float,
    crate_filter: Optional[str],
) -> str:
    """Render the full markdown report."""
    out: List[str] = []

    # ---- Header ----
    out.append("# synapse-rust 测试覆盖率分析报告")
    out.append("")
    out.append(f"- 数据来源: `coverage/lcov.info`")
    out.append(f"- 阈值: 达标 ≥ {threshold:.0f}% | 警告 ≥ {warn_floor:.0f}% | 不达标 < {warn_floor:.0f}% | 零覆盖 LH=0")
    if crate_filter:
        out.append(f"- Crate 过滤: `{crate_filter}`")
    out.append("")

    if not files:
        out.append("_无符合条件的数据_")
        return "\n".join(out)

    # ---- 整体统计 ----
    total_files = len(files)
    total_lf = sum(f.lf for f in files)
    total_lh = sum(f.lh for f in files)
    overall_pct = (total_lh / total_lf * 100.0) if total_lf else 0.0

    pass_files = [f for f in files if f.status == "PASS"]
    warn_files = [f for f in files if f.status == "WARN"]
    fail_files = [f for f in files if f.status == "FAIL"]
    zero_files = [f for f in files if f.status == "ZERO"]

    out.append("## 1. 整体统计")
    out.append("")
    out.append("| 指标 | 值 |")
    out.append("|------|----|")
    out.append(f"| 总文件数 | {total_files} |")
    out.append(f"| 总行数 (LF) | {total_lf:,} |")
    out.append(f"| 命中行数 (LH) | {total_lh:,} |")
    out.append(f"| 整体行覆盖率 | **{overall_pct:.2f}%** |")
    out.append(f"| ✅ 达标文件 (≥{threshold:.0f}%) | {len(pass_files)} ({len(pass_files)/total_files*100:.1f}%) |")
    out.append(f"| ⚠️ 警告文件 ({warn_floor:.0f}-{threshold:.0f}%) | {len(warn_files)} ({len(warn_files)/total_files*100:.1f}%) |")
    out.append(f"| ❌ 不达标文件 (<{warn_floor:.0f}%) | {len(fail_files)} ({len(fail_files)/total_files*100:.1f}%) |")
    out.append(f"| ⛔ 零覆盖文件 (LH=0) | {len(zero_files)} ({len(zero_files)/total_files*100:.1f}%) |")
    out.append("")

    # ---- 按 crate 分组 ----
    crate_stats: Dict[str, CrateStats] = defaultdict(lambda: CrateStats(crate=""))
    for f in files:
        cs = crate_stats[f.crate]
        cs.crate = f.crate
        cs.files.append(f)
        cs.total_lf += f.lf
        cs.total_lh += f.lh

    out.append("## 2. 按 Crate 分组统计")
    out.append("")
    out.append("| Crate | 文件数 | 总行数 | 命中行数 | 加权覆盖率 | 平均覆盖率 | 不达标+零覆盖 |")
    out.append("|-------|--------|--------|----------|-----------|-----------|---------------|")
    for crate_name in sorted(crate_stats.keys()):
        cs = crate_stats[crate_name]
        failing = len(cs.failing_files)
        out.append(
            f"| {crate_name} | {len(cs.files)} | {cs.total_lf:,} | {cs.total_lh:,} | "
            f"{cs.avg_pct:.2f}% | {cs.mean_pct:.2f}% | {failing} |"
        )
    out.append("")

    # ---- 未达标文件清单 ----
    failing_all = sorted(
        [f for f in files if f.status in ("FAIL", "ZERO")],
        key=lambda f: (f.pct, f.rel_path),
    )

    out.append("## 3. 未达标文件清单 (按覆盖率升序)")
    out.append("")
    if not failing_all:
        out.append("_所有文件均达标_")
        out.append("")
    else:
        out.append("| # | 状态 | 文件路径 | 覆盖率 | 总行数 LF | 命中行数 LH |")
        out.append("|---|------|----------|--------|-----------|-------------|")
        for i, f in enumerate(failing_all, 1):
            out.append(
                f"| {i} | {f.status_icon} | `{f.rel_path}` | {f.pct:.2f}% | {f.lf} | {f.lh} |"
            )
        out.append("")

    # ---- 警告文件清单 ----
    warn_all = sorted(
        [f for f in files if f.status == "WARN"],
        key=lambda f: (f.pct, f.rel_path),
    )
    out.append("## 4. 警告文件清单 (70% ≤ 覆盖率 < 80%)")
    out.append("")
    if not warn_all:
        out.append("_无警告文件_")
        out.append("")
    else:
        out.append("| # | 文件路径 | 覆盖率 | 总行数 LF | 命中行数 LH |")
        out.append("|---|----------|--------|-----------|-------------|")
        for i, f in enumerate(warn_all, 1):
            out.append(
                f"| {i} | `{f.rel_path}` | {f.pct:.2f}% | {f.lf} | {f.lh} |"
            )
        out.append("")

    # ---- 每个 crate 中覆盖率最低的 5 个文件 ----
    out.append("## 5. 重点模块摘要 (每个 Crate 覆盖率最低的 5 个文件)")
    out.append("")
    for crate_name in sorted(crate_stats.keys()):
        cs = crate_stats[crate_name]
        out.append(f"### {crate_name}")
        out.append("")
        worst = sorted(cs.files, key=lambda f: (f.pct, -f.lf))[:5]
        if not worst:
            out.append("_无数据_")
            out.append("")
            continue
        out.append("| 状态 | 文件路径 | 覆盖率 | 总行数 | 命中行数 |")
        out.append("|------|----------|--------|--------|----------|")
        for f in worst:
            out.append(
                f"| {f.status_icon} | `{f.rel_path}` | {f.pct:.2f}% | {f.lf} | {f.lh} |"
            )
        out.append("")

    # ---- 关键模块交叉对照 (TDD 落地清单) ----
    key_modules = [
        ("synapse-storage", "event", ["synapse-storage/src/event/"]),
        ("synapse-storage", "room", ["synapse-storage/src/room"]),
        ("synapse-storage", "membership", ["synapse-storage/src/membership"]),
        ("synapse-storage", "presence", ["synapse-storage/src/presence"]),
        ("synapse-storage", "user", ["synapse-storage/src/user"]),
        ("synapse-storage", "device", ["synapse-storage/src/device"]),
        ("synapse-services", "auth", ["synapse-services/src/auth/"]),
        ("synapse-services", "sync_service", ["synapse-services/src/sync_service"]),
        ("synapse-services", "sliding_sync_service", ["synapse-services/src/sliding_sync_service"]),
        ("synapse-services", "friend_room_service", ["synapse-services/src/friend_room_service"]),
        ("synapse-federation", "client", ["synapse-federation/src/client.rs"]),
        ("synapse-federation", "event_auth", ["synapse-federation/src/event_auth"]),
        ("synapse-federation", "device_sync", ["synapse-federation/src/device_sync.rs"]),
        ("src/web/routes", "login", ["src/web/routes/auth.rs", "src/web/routes/auth/"]),
        ("src/web/routes", "register", ["src/web/routes/register.rs"]),
        ("src/web/routes", "sync", ["src/web/routes/sync.rs"]),
        ("src/web/routes", "profile", ["src/web/routes/profile"]),
        ("src/web/routes", "room_summary", ["src/web/routes/room_summary.rs"]),
    ]

    out.append("## 6. TDD 落地清单关键模块交叉对照")
    out.append("")
    out.append("| Crate | 模块 | 文件状态 | 备注 |")
    out.append("|-------|------|----------|------|")
    for crate_name, mod_name, prefixes in key_modules:
        matched = [f for f in files if any(f.rel_path.startswith(p) for p in prefixes)]
        if not matched:
            out.append(f"| {crate_name} | {mod_name} | ⛔ 无覆盖率数据 | lcov.info 中未找到匹配文件 (前缀: {', '.join(prefixes)}) |")
            continue
        agg_lf = sum(f.lf for f in matched)
        agg_lh = sum(f.lh for f in matched)
        agg_pct = (agg_lh / agg_lf * 100.0) if agg_lf else 0.0
        if agg_lh == 0:
            status = "⛔ 零覆盖"
        elif agg_pct >= threshold:
            status = "✅ 达标"
        elif agg_pct >= warn_floor:
            status = "⚠️ 警告"
        else:
            status = "❌ 不达标"
        file_list = ", ".join(f"`{f.rel_path}`" for f in matched)
        out.append(
            f"| {crate_name} | {mod_name} | {status} {agg_pct:.2f}% ({agg_lh}/{agg_lf}) | {file_list} |"
        )
    out.append("")

    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Analyze lcov.info and emit a Markdown coverage report."
    )
    parser.add_argument(
        "--lcov", type=pathlib.Path, default=DEFAULT_LCOV,
        help=f"Path to lcov.info (default: {DEFAULT_LCOV})",
    )
    parser.add_argument(
        "--threshold", type=float, default=80.0,
        help="Pass threshold (default: 80)",
    )
    parser.add_argument(
        "--warn-floor", type=float, default=70.0,
        help="Warn floor / global threshold (default: 70)",
    )
    parser.add_argument(
        "--crate", type=str, default=None,
        help="Filter to a specific crate (e.g. synapse-storage, 'main (src/)')",
    )
    args = parser.parse_args()

    try:
        files = parse_lcov(args.lcov, args.threshold, args.warn_floor)
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

    if args.crate:
        if args.crate in ("src", "main"):
            args.crate = "main (src/)"
        files = [f for f in files if f.crate == args.crate]

    report = render_markdown(files, args.threshold, args.warn_floor, args.crate)
    print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
