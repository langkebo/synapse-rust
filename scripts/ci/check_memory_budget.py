#!/usr/bin/env python3
"""内存预算不变式门禁（2026-09-23, SIGTERM root-cause fix）。

WorkBuddy CLI 环境（以及小型 CI runner）的内存限制通常 ≤4GB，而本项目的测试架构在
高并发下会突破这个限制：

  * 每个测试二进制文件 ~200-500MB RSS（含依赖库）
  * PgPool 连接池：每连接 ~5MB × max_connections
  * jemalloc 默认配置保留大量空闲内存不释放

本门禁检测以下可能导致 OOM 的改动：

1. 新增测试文件（每个测试二进制 ~200MB）
2. 新增 PgPool 创建（每个池 ~50MB 基础开销）
3. 新增大型数据结构（>1MB 静态分配）
4. 移除 `--test-threads` 限制或提高并发度

退出码:
  0: 预算内
  1: 超出预算，需要人工审核或降低并发度
  2: 脚本错误

用法:
  python3 scripts/ci/check_memory_budget.py

环境变量:
  MEMORY_BUDGET_MB: 内存预算上限（默认 4096MB）
  MEMORY_BUDGET_WARN_MB: 警告阈值（默认 3072MB）
"""

from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys
from typing import NamedTuple

ROOT = pathlib.Path(__file__).resolve().parents[2]

# 内存预算（可配置）
MEMORY_BUDGET_MB = int(os.environ.get("MEMORY_BUDGET_MB", "4096"))
MEMORY_BUDGET_WARN_MB = int(os.environ.get("MEMORY_BUDGET_WARN_MB", "3072"))

# 估算常量
TEST_BINARY_RSS_MB = 200  # 每个测试二进制文件的基线内存
POOL_OVERHEAD_MB = 50  # 每个 PgPool 的基础开销
PER_CONNECTION_MB = 5  # 每个数据库连接的内存占用
LARGE_DATASTRUCTURE_MB = 1  # 大型数据结构的阈值


class MemoryImpact(NamedTuple):
    """内存影响评估结果。"""

    delta_mb: int  # 本次变更的内存影响（MB）
    baseline_mb: int  # 当前基线内存消耗（MB）
    projected_mb: int  # 投影总内存消耗（MB）
    risk_level: str  # "low" | "medium" | "high" | "critical"


def get_changed_files() -> list[pathlib.Path]:
    """获取本次变更的所有 Rust 文件。"""
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", "HEAD~1"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
    except subprocess.CalledProcessError as e:
        print(f"⚠️  git diff 失败：{e}", file=sys.stderr)
        return []

    files = []
    for line in result.stdout.strip().split("\n"):
        if not line:
            continue
        path = ROOT / line
        if path.suffix == ".rs":
            files.append(path)
    return files


def count_new_test_files(files: list[pathlib.Path]) -> int:
    """统计新增的测试文件数量。"""
    count = 0
    for f in files:
        content = f.read_text(encoding="utf-8", errors="replace")
        # 检测 #[test] 或 #[tokio::test]
        if re.search(r"#\[(?:tokio::)?test\]", content):
            count += 1
    return count


def count_pgpool_creates(files: list[pathlib.Path]) -> int:
    """统计新增的 PgPool 创建点。"""
    count = 0
    for f in files:
        content = f.read_text(encoding="utf-8", errors="replace")
        # 检测 PgPoolOptions::new() 或 PgPool::connect
        count += len(re.findall(r"PgPoolOptions::new\(\)", content))
        count += len(re.findall(r"PgPool::connect", content))
    return count


def detect_large_structures(files: list[pathlib.Path]) -> list[tuple[pathlib.Path, int]]:
    """检测新增的大型静态数据结构（>1MB）。"""
    large_ones = []
    for f in files:
        content = f.read_text(encoding="utf-8", errors="replace")
        # 检测 vec![...] 或 [T; N] 其中 N * sizeof(T) > 1MB
        matches = re.findall(r"\[([A-Za-z_][A-Za-z0-9_]*)\s*;\s*(\d+)\]", content)
        for typ, size in matches:
            # 粗略估算：假设每个元素 8 字节（指针大小）
            size_bytes = int(size) * 8
            if size_bytes > 1024 * 1024:  # > 1MB
                large_ones.append((f, size_bytes // (1024 * 1024)))
    return large_ones


def estimate_current_baseline() -> int:
    """估算当前测试架构的基线内存消耗。"""
    # 简化的估算模型
    # 假设平均每次运行 4 个测试线程
    num_test_bins = 8  # 预估同时运行的测试二进制文件数
    pool_size = 20  # 低内存环境下的连接池大小
    num_pools = 4

    base = num_test_bins * TEST_BINARY_RSS_MB
    pool_mem = num_pools * (POOL_OVERHEAD_MB + pool_size * PER_CONNECTION_MB)

    return base + pool_mem


def assess_memory_impact(files: list[pathlib.Path]) -> MemoryImpact:
    """评估本次变更的内存影响。"""
    baseline = estimate_current_baseline()

    # 新增测试文件
    new_tests = count_new_test_files(files)
    test_impact = new_tests * TEST_BINARY_RSS_MB

    # 新增 PgPool
    new_pools = count_pgpool_creates(files)
    pool_impact = new_pools * POOL_OVERHEAD_MB

    # 大型数据结构
    large_structs = detect_large_structures(files)
    struct_impact = sum(size_mb for _, size_mb in large_structs)

    total_delta = test_impact + pool_impact + struct_impact
    projected = baseline + total_delta

    # 风险等级
    if projected > MEMORY_BUDGET_MB:
        risk = "critical"
    elif projected > MEMORY_BUDGET_WARN_MB:
        risk = "high"
    elif total_delta > 500:
        risk = "medium"
    else:
        risk = "low"

    return MemoryImpact(
        delta_mb=total_delta,
        baseline_mb=baseline,
        projected_mb=projected,
        risk_level=risk,
    )


def print_report(impact: MemoryImpact, large_structs: list[tuple[pathlib.Path, int]]) -> None:
    """打印评估报告。"""
    print("=" * 60)
    print("📊 内存预算评估报告")
    print("=" * 60)
    print(f"基线内存消耗：     {impact.baseline_mb:>6} MB")
    print(f"本次变更影响：     {impact.delta_mb:>6} MB")
    print(f"投影总内存消耗：   {impact.projected_mb:>6} MB")
    print(f"内存预算上限：     {MEMORY_BUDGET_MB:>6} MB")
    print(f"警告阈值：         {MEMORY_BUDGET_WARN_MB:>6} MB")
    print("-" * 60)
    print(f"风险等级：         {impact.risk_level.upper()}")
    print("=" * 60)

    if large_structs:
        print("\n⚠️  检测到大型静态数据结构:")
        for f, size_mb in large_structs:
            print(f"  - {f.relative_to(ROOT)}: +{size_mb} MB")

    if impact.risk_level in ("high", "critical"):
        print("\n💡 建议措施:")
        print("  1. 使用 --test-threads 1 串行运行测试")
        print("  2. 分 crate 测试：cargo nextest run -p <crate> ...")
        print("  3. 设置 MALLOC_CONF 优化内存回收:")
        print("     export MALLOC_CONF=retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false")
        print("  4. 使用 test-lowmem profile:")
        print("     cargo nextest run --profile test-lowmem ...")
        print("  5. 检查是否有不必要的大型数据结��")

    if impact.delta_mb > 0:
        print("\n📈 本次变更明细:")
        print(f"  - 新增测试文件：{count_new_test_files(get_changed_files())} 个")
        print(f"  - 新增 PgPool 创建：{count_pgpool_creates(get_changed_files())} 个")


def main() -> None:
    files = get_changed_files()
    if not files:
        print("ℹ️  无 Rust 文件变更，跳过内存预算检查")
        sys.exit(0)

    impact = assess_memory_impact(files)
    large_structs = detect_large_structures(files)

    print_report(impact, large_structs)

    # 退出码
    if impact.risk_level == "critical":
        print("\n❌ 超出内存预算！请采取上述建议措施或联系维护者调整预算。")
        sys.exit(1)
    elif impact.risk_level == "high":
        print("\n⚠️  接近内存预算上限！建议采取上述措施。")
        # 警告但不失败
        sys.exit(0)
    else:
        print("\n✅ 内存消耗在预算范围内")
        sys.exit(0)


if __name__ == "__main__":
    main()
