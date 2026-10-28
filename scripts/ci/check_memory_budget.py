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

基线内存不再硬编码在脚本里（PERF-05），而是由 `scripts/ci/memory_budget_baseline`
记录的**测试架构参数**按模型推导；文件缺失/缺项即失败（exit 2），不静默通过。
基线随测试架构演进更新——改基线文件并写明理由，而不是改脚本里的字面量。

退出码:
  0: 预算内
  1: 达到 high 或 critical 风险（两者都阻断 PR；此前 high 只警告不失败，是 PERF-05 的缺口）
  2: 脚本错误（含基线文件缺失/缺项）

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

# 基线：记录的测试架构参数（NUM_TEST_BINS / NUM_POOLS / POOL_SIZE），
# 见 scripts/ci/memory_budget_baseline。
BASELINE_FILE = ROOT / "scripts" / "ci" / "memory_budget_baseline"

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


def get_added_files() -> set[str]:
    """相对 HEAD~1 **新增**（added）的路径集合；失败时返回空集。"""
    try:
        result = subprocess.run(
            ["git", "diff", "--name-only", "--diff-filter=A", "HEAD~1"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
    except subprocess.CalledProcessError:
        return set()
    return {line for line in result.stdout.strip().split("\n") if line}


# `tests/foo.rs` / `benches/foo.rs`（直接位于目录下）才会各自成一个测试二进制。
# `tests/unit/foo_tests.rs` 是被 `tests/unit/mod.rs` 用 `mod` 吸纳进去的，**不新建**二进制。
_DIRECT_TEST_TARGET = re.compile(r"(?:^|/)(?:tests|benches)/[^/]+\.rs$")


def count_new_test_files(files: list[pathlib.Path]) -> int:
    """统计**真的会多出一个测试二进制**的文件数量。

    ## 旧判据与它的假阳（2026-10-28）

    旧实现是「改动的 `.rs` 里出现 `#[test]` 就算一个新增测试二进制」，然后按
    `TEST_BINARY_RSS_MB`（~200MB）计入预算。这把两类**不新增二进制**的改动也算了进去：

    * 在一个已有的测试模块里**加一个用例**（文件还是那个文件、二进制还是那个二进制）；
    * 把测试从一个文件**搬到另一个文件** —— 反而会算成 +2。

    实测：把 `db_schema_pool_tests` 拆出 `lib.rs`（纯移动 + 一处新用例）之后，
    `Check memory budget` 在 `Test & Lint (1.93.0/stale, default-features)` **两条车道同时红**，
    而那次改动一个二进制都没多出来。一个"防止 OOM"的门禁把重构当成 OOM 风险，
    结果就是没人再敢拆文件 —— 而拆文件正是 `check_file_size_ratchet.py` 要求的动作。

    现在只认**新增**且**直接位于 `tests/` 或 `benches/` 下**的 `.rs`：
    那是 cargo 真的会为它建一个 target 的形态（`[[test]]` 在 Cargo.toml 里显式声明的
    情形本门禁看不到，属于已知的漏报方向，宁可漏报也不误报）。
    """
    added = get_added_files()
    count = 0
    for f in files:
        try:
            rel = f.relative_to(ROOT).as_posix()
        except ValueError:
            continue
        if rel not in added:
            continue
        if not _DIRECT_TEST_TARGET.search(rel):
            continue
        content = f.read_text(encoding="utf-8", errors="replace")
        # 检测 #[test] / #[tokio::test] / bench harness
        if re.search(r"#\[(?:tokio::)?test\]", content) or "harness" in content:
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


def detect_large_structures(
    files: list[pathlib.Path],
) -> list[tuple[pathlib.Path, int]]:
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


def read_baseline_inputs() -> dict[str, int]:
    """读取记录的测试架构参数（缺失即失败，不允许静默回落到硬编码）。"""
    if not BASELINE_FILE.exists():
        print(
            f"❌ 缺少内存基线文件：{BASELINE_FILE.relative_to(ROOT)}。"
            "内存门禁必须读记录在案的基线，缺失即失败（不允许静默通过）。",
            file=sys.stderr,
        )
        sys.exit(2)
    values: dict[str, int] = {}
    for line in BASELINE_FILE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if "=" in line:
            key, _, value = line.partition("=")
            try:
                values[key.strip()] = int(value.strip())
            except ValueError:
                print(
                    f"❌ {BASELINE_FILE.relative_to(ROOT)} 的 `{line}` 不是 KEY=整数 形式",
                    file=sys.stderr,
                )
                sys.exit(2)
    required = ("NUM_TEST_BINS", "NUM_POOLS", "POOL_SIZE")
    missing = [k for k in required if k not in values]
    if missing:
        print(
            f"❌ {BASELINE_FILE.relative_to(ROOT)} 缺少基线项：{', '.join(missing)}",
            file=sys.stderr,
        )
        sys.exit(2)
    return values


def estimate_current_baseline() -> int:
    """按记录在案的测试架构参数推导基线内存消耗（基线随架构演进更新）。"""
    b = read_baseline_inputs()
    base = b["NUM_TEST_BINS"] * TEST_BINARY_RSS_MB
    pool_mem = b["NUM_POOLS"] * (POOL_OVERHEAD_MB + b["POOL_SIZE"] * PER_CONNECTION_MB)
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


def print_report(
    impact: MemoryImpact, large_structs: list[tuple[pathlib.Path, int]]
) -> None:
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
        print(
            "     export MALLOC_CONF=retain:true,dirty_decay_ms:1000,narenas:2,background_thread:false"
        )
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
        # PERF-05：high 也必须阻断——此前只警告不失败，内存回归会被放过。
        print("\n❌ 达到高内存风险（high）！请采取上述措施或联系维护者调整预算。")
        sys.exit(1)
    else:
        print("\n✅ 内存消耗在预算范围内")
        sys.exit(0)


if __name__ == "__main__":
    main()
