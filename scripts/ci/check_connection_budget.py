#!/usr/bin/env python3
"""连接预算不变式门禁（DB review 优先级 7）。

PostgreSQL 的 `max_connections` 是**全局**资源，而每个 synapse 进程都会开一个连接池
（上限 = `database.max_size`，默认 50）。两侧都没有护栏时，多开一个进程就会让
「连接耗尽」变成运行期随机失败——仓库历史上「1408 passed + 12 failed + 7 timed out」
（并发争用）与测试池 starvation 都是这个根因。

本门禁做两件事：
  1. 从源码读出生效的池上限（`default_database_max_size()`），避免脚本与代码漂移；
  2. 校验 `docker/deploy/README.md` 里**成文**的部署预算表，断言
        pool × 进程数 + 保留 ≤ max_connections
     若预算表缺失或算不平 → 失败。数字缺失时**必须报错**，不能静默通过。

用法：python3 scripts/ci/check_connection_budget.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
DB_CONFIG = ROOT / "synapse-common" / "src" / "config" / "database.rs"
BUDGET_DOC = ROOT / "docker" / "deploy" / "README.md"


def effective_pool_per_process() -> int:
    text = DB_CONFIG.read_text(encoding="utf-8", errors="replace")
    m = re.search(r"fn default_database_max_size\(\) -> u32 \{\s*(\d+)", text)
    if not m:
        print(
            f"❌ 无法从 {DB_CONFIG.relative_to(ROOT)} 解析 default_database_max_size()",
            file=sys.stderr,
        )
        sys.exit(2)
    return int(m.group(1))


def documented_budget() -> dict[str, int]:
    """Parse the budget table from docker/deploy/README.md."""
    if not BUDGET_DOC.exists():
        print(f"❌ 缺少连接预算文档：{BUDGET_DOC.relative_to(ROOT)}", file=sys.stderr)
        sys.exit(2)
    text = BUDGET_DOC.read_text(encoding="utf-8", errors="replace")
    keys = {
        "pool_per_process": r"每进程池上限\s*\|\s*(\d+)",
        "max_processes": r"最大并发进程数\s*\|\s*(\d+)",
        "reserve": r"测试/管理保留\s*\|\s*(\d+)",
        "max_connections": r"PostgreSQL max_connections\s*\|\s*(\d+)",
    }
    out: dict[str, int] = {}
    for name, pat in keys.items():
        m = re.search(pat, text)
        if not m:
            print(
                f"❌ {BUDGET_DOC.relative_to(ROOT)} 缺少连接预算项 `{name}`。"
                "连接预算是必须成文的部署约束，缺失即失败（不允许静默通过）。",
                file=sys.stderr,
            )
            sys.exit(2)
        out[name] = int(m.group(1))
    return out


def main() -> int:
    pool = effective_pool_per_process()
    doc = documented_budget()

    if doc["pool_per_process"] != pool:
        print(
            f"❌ 文档与代码漂移：docker/deploy/README.md 写每进程池上限 = {doc['pool_per_process']}，"
            f"而 database.rs 的 default_database_max_size() = {pool}。",
            file=sys.stderr,
        )
        return 1

    needed = pool * doc["max_processes"] + doc["reserve"]
    print(
        f"连接预算：每进程 {pool} × 进程 {doc['max_processes']} + 保留 {doc['reserve']} "
        f"= {needed}，PostgreSQL max_connections = {doc['max_connections']}"
    )
    if needed > doc["max_connections"]:
        print(
            f"❌ 超预算 {needed} > {doc['max_connections']}：连接耗尽会表现为随机失败"
            "（测试 starvation / 部署期 500）。请提高 max_connections、降低 database.max_size，"
            "或减少并发进程数，并同步更新预算表。",
            file=sys.stderr,
        )
        return 1
    print(
        f"✅ 连接预算成立：{needed} ≤ {doc['max_connections']}（余量 {doc['max_connections'] - needed}）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
