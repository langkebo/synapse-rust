#!/usr/bin/env python3
"""连接预算不变式门禁（DB review 优先级 7 / PERF-04）。

PostgreSQL 的 `max_connections` 是**全局**资源，而每个 synapse 进程都会开一个连接池
（上限 = `database.max_size`，默认 50）。两侧都没有护栏时，多开一个进程就会让
「连接耗尽」变成运行期随机失败——仓库历史上「1408 passed + 12 failed + 7 timed out」
（并发争用）与测试池 starvation 都是这个根因。

本门禁做四件事：
  1. 从源码读出生效的池上限（`default_database_max_size()`），避免脚本与代码漂移；
  2. 从**真实运行配置**读出生效的池上限与 `max_connections`
     （`docker/config/homeserver.yaml` 的 `database.max_size`、
       `docker/deploy/docker-compose.yml` 的 `SYNAPSE__DATABASE__MAX_SIZE` 默认值、
       `docker/config/postgres.conf` 的 `max_connections`），
     断言「代码默认 == 运行 yaml == compose 默认」——三者漂移即失败；
  3. 断言成文文档表 == 代码 == 运行配置（`max_connections` 对 `postgres.conf`）。
     历史上文档写 250 而运行 `postgres.conf` 实为 200（PERF-04），门禁只校验文档
     会放过真实超预算 → 假绿；本步即为堵这个缺口；
  4. 在**真实运行配置**上校验不变式
        pool × 进程数 + 保留 ≤ max_connections
     数字缺失时**必须报错**（exit 2），不能静默通过。

用法：python3 scripts/ci/check_connection_budget.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
DB_CONFIG = ROOT / "synapse-common" / "src" / "config" / "database.rs"
HOMESERVER_YAML = ROOT / "docker" / "config" / "homeserver.yaml"
POSTGRES_CONF = ROOT / "docker" / "config" / "postgres.conf"
COMPOSE = ROOT / "docker" / "deploy" / "docker-compose.yml"
BUDGET_DOC = ROOT / "docker" / "deploy" / "README.md"


def _read(path: pathlib.Path) -> str:
    if not path.exists():
        print(f"❌ 缺少运行配置文件：{path.relative_to(ROOT)}", file=sys.stderr)
        sys.exit(2)
    return path.read_text(encoding="utf-8", errors="replace")


def _fail_missing(path: pathlib.Path, what: str) -> None:
    print(
        f"❌ 无法从 {path.relative_to(ROOT)} 解析 {what}。"
        "连接预算门禁必须读取真实运行配置，缺失即失败（不允许静默通过）。",
        file=sys.stderr,
    )
    sys.exit(2)


def code_pool_per_process() -> int:
    """从源码读出生效的池上限（唯一代码真相源）。"""
    text = _read(DB_CONFIG)
    m = re.search(r"fn default_database_max_size\(\) -> u32 \{\s*(\d+)", text)
    if not m:
        _fail_missing(DB_CONFIG, "default_database_max_size()")
    return int(m.group(1))


def yaml_pool_per_process() -> int:
    """从 docker homeserver.yaml 的 `database:` 块读 `max_size`。"""
    text = _read(HOMESERVER_YAML)
    # 只在顶层 `database:` 块内匹配，避免命中 redis.pool_size 等其它键。
    block = re.search(r"^database:[^\n]*\n((?:[ \t]+[^\n]*\n|\n)*)", text, re.MULTILINE)
    if not block:
        _fail_missing(HOMESERVER_YAML, "顶层 `database:` 块")
    m = re.search(r"^[ \t]+max_size:[ \t]*(\d+)", block.group(1), re.MULTILINE)
    if not m:
        _fail_missing(HOMESERVER_YAML, "`database.max_size`")
    return int(m.group(1))


def compose_pool_per_process() -> int:
    """从 docker-compose.yml 读 `SYNAPSE__DATABASE__MAX_SIZE` 的生效默认值。"""
    text = _read(COMPOSE)
    m = re.search(
        r"SYNAPSE__DATABASE__MAX_SIZE:[ \t]*\$\{SYNAPSE__DATABASE__MAX_SIZE:-(\d+)\}",
        text,
    )
    if not m:
        # 也接受直接写字面量（未使用 env 默认值语法）的形式
        m = re.search(r"SYNAPSE__DATABASE__MAX_SIZE:[ \t]*(\d+)", text)
    if not m:
        _fail_missing(COMPOSE, "`SYNAPSE__DATABASE__MAX_SIZE` 默认值")
    return int(m.group(1))


def runtime_max_connections() -> int:
    """从 canonical postgres.conf 读 `max_connections`（真实运行值）。"""
    text = _read(POSTGRES_CONF)
    m = re.search(r"^max_connections[ \t]*=[ \t]*(\d+)", text, re.MULTILINE)
    if not m:
        _fail_missing(POSTGRES_CONF, "`max_connections`")
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
    code_pool = code_pool_per_process()
    yaml_pool = yaml_pool_per_process()
    compose_pool = compose_pool_per_process()
    runtime_max = runtime_max_connections()
    doc = documented_budget()

    # 1) 三个池上限来源必须一致（代码默认 / 运行 yaml / compose env 默认）
    if len({code_pool, yaml_pool, compose_pool}) != 1:
        print(
            "❌ 池上限来源漂移："
            f"database.rs default={code_pool}，"
            f"homeserver.yaml database.max_size={yaml_pool}，"
            f"docker-compose.yml SYNAPSE__DATABASE__MAX_SIZE 默认={compose_pool}。"
            "三者必须一致。",
            file=sys.stderr,
        )
        return 1

    # 2) 文档表必须与代码一致
    if doc["pool_per_process"] != code_pool:
        print(
            f"❌ 文档与代码漂移：docker/deploy/README.md 写每进程池上限 = {doc['pool_per_process']}，"
            f"而 database.rs 的 default_database_max_size() = {code_pool}。",
            file=sys.stderr,
        )
        return 1

    # 3) 文档表的 max_connections 必须等于真实运行配置（PERF-04 核心断言）
    if doc["max_connections"] != runtime_max:
        print(
            f"❌ 文档与运行配置漂移：docker/deploy/README.md 写 PostgreSQL max_connections = "
            f"{doc['max_connections']}，而 docker/config/postgres.conf 实际为 {runtime_max}。"
            "只校验文档会放过真实超预算（假绿），两者必须一致。",
            file=sys.stderr,
        )
        return 1

    # 4) 在真实运行配置上校验不变式
    needed = yaml_pool * doc["max_processes"] + doc["reserve"]
    print(
        f"连接预算：每进程 {yaml_pool}（代码/运行一致）× 进程 {doc['max_processes']} "
        f"+ 保留 {doc['reserve']} = {needed}，运行 postgres max_connections = {runtime_max}"
    )
    if needed > runtime_max:
        print(
            f"❌ 超预算 {needed} > {runtime_max}：连接耗尽会表现为随机失败"
            "（测试 starvation / 部署期 500）。请提高 max_connections、降低 database.max_size，"
            "或减少并发进程数，并同步更新预算表。",
            file=sys.stderr,
        )
        return 1
    print(
        f"✅ 连接预算成立：{needed} ≤ {runtime_max}（余量 {runtime_max - needed}）；"
        "文档 == 代码 == 运行配置"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
