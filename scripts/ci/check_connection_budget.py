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


AUX_PROCESSES = 7
"""PostgreSQL 常驻**非连接**进程数（实测，docs/audit/DB_REVIEW_2026-10-15.md §4）：
postmaster / logger / checkpointer / background writer / walwriter / autovacuum launcher /
logical replication launcher。并行 worker 另计（`max_worker_processes`），它们同样占 pid。"""

MEM_BUDGET_RATIO = 0.8
"""内存预算只允许用掉容器 `mem_limit` 的 80%，其余留给页缓存与瞬时尖峰。"""


def _mb(value: str) -> int:
    """把 `256MB` / `1536m` / `4MB` 这类写法统一成 MB 整数。"""
    m = re.fullmatch(r"(\d+)\s*([Mm][Bb]?)?", value.strip())
    if not m:
        raise ValueError(f"无法解析内存量：{value!r}")
    return int(m.group(1))


def runtime_pids_limit() -> int:
    """从真实运行 compose 读 PostgreSQL 容器的 `pids_limit`（deploy 栈是权威）。"""
    text = _read(COMPOSE)
    m = re.search(r"POSTGRES_PIDS_LIMIT:-(\d+)", text)
    if not m:
        m = re.search(r"^\s*pids_limit:[ \t]*(\d+)", text, re.MULTILINE)
    if not m:
        _fail_missing(COMPOSE, "`POSTGRES_PIDS_LIMIT` 默认值")
    return int(m.group(1))


def runtime_mem_limit_mb() -> int:
    """从真实运行 compose 读 PostgreSQL 容器的 `mem_limit`（MB）。"""
    text = _read(COMPOSE)
    m = re.search(r"POSTGRES_MEMORY_LIMIT:-(\d+m)", text)
    if not m:
        _fail_missing(COMPOSE, "`POSTGRES_MEMORY_LIMIT` 默认值")
    return _mb(m.group(1))


def pg_conf_mb(param: str) -> int:
    """从 canonical postgres.conf 读内存类参数（统一为 MB）。"""
    text = _read(POSTGRES_CONF)
    m = re.search(rf"^{param}[ \t]*=[ \t]*([0-9]+[A-Za-z]*)", text, re.MULTILINE)
    if not m:
        _fail_missing(POSTGRES_CONF, f"`{param}`")
    return _mb(m.group(1))


def pg_conf_int(param: str, default: int) -> int:
    """从 canonical postgres.conf 读整型参数；文件里没写就走 PG 默认值。"""
    text = _read(POSTGRES_CONF)
    m = re.search(rf"^{param}[ \t]*=[ \t]*(\d+)", text, re.MULTILINE)
    return int(m.group(1)) if m else default


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
        "pids_limit": r"PostgreSQL pids_limit\s*\|\s*(\d+)",
        "work_mem_concurrency": r"并发排序/哈希节点数[^|]*\|\s*(\d+)",
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

    # 5) pids 预算（DB_REVIEW_2026-10-15 §4）：连接数够了但进程数不够，一样连不上。
    #    `max_connections` 只算"连接进程"，不含 postmaster/checkpointer 等常驻进程与并行 worker。
    pids_limit = runtime_pids_limit()
    if doc["pids_limit"] != pids_limit:
        print(
            f"❌ 文档与运行配置漂移：docker/deploy/README.md 写 PostgreSQL pids_limit = "
            f"{doc['pids_limit']}，而 docker/deploy/docker-compose.yml 实际为 {pids_limit}。",
            file=sys.stderr,
        )
        return 1
    workers = pg_conf_int("max_worker_processes", 8)
    pids_needed = runtime_max + AUX_PROCESSES + workers
    print(
        f"pids 预算：max_connections {runtime_max} + 常驻进程 {AUX_PROCESSES} + 并行 worker {workers} "
        f"= {pids_needed}，运行 pids_limit = {pids_limit}"
    )
    if pids_needed > pids_limit:
        print(
            f"❌ pids 超预算 {pids_needed} > {pids_limit}：连接打满后段会 "
            "`could not fork new process` —— 客户端看到的是**连接失败**而不是排队（极难归因）。"
            "请提高 POSTGRES_PIDS_LIMIT / DB_PIDS_LIMIT，或降低 max_connections，并同步预算表。",
            file=sys.stderr,
        )
        return 1

    # 6) 内存预算（DB_REVIEW_2026-10-15 §5）：`work_mem` 是**每排序/哈希节点**上限，不是每连接上限。
    #    按 max_connections 算最坏情况会得出必然打穿的数，故用文档里**显式登记**的"预期并发"。
    shared_buffers = pg_conf_mb("shared_buffers")
    work_mem = pg_conf_mb("work_mem")
    maintenance = pg_conf_mb("maintenance_work_mem")
    mem_limit = runtime_mem_limit_mb()
    concurrency = doc["work_mem_concurrency"]
    mem_needed = shared_buffers + work_mem * concurrency + maintenance
    mem_budget = int(mem_limit * MEM_BUDGET_RATIO)
    print(
        f"内存预算：shared_buffers {shared_buffers}MB + work_mem {work_mem}MB × 预期并发 {concurrency} "
        f"+ maintenance {maintenance}MB = {mem_needed}MB，上限 = mem_limit {mem_limit}MB × "
        f"{MEM_BUDGET_RATIO:.0%} = {mem_budget}MB"
    )
    if mem_needed > mem_budget:
        print(
            f"❌ 内存超预算 {mem_needed}MB > {mem_budget}MB：cgroup OOM 杀的是 **postmaster** ⇒ "
            "整库重启（而不是「杀掉一个慢查询」）。请下调 work_mem / 上调 POSTGRES_MEMORY_LIMIT，"
            "或下调文档里的预期并发并说明理由。",
            file=sys.stderr,
        )
        return 1

    print(
        f"✅ pids 预算成立：{pids_needed} ≤ {pids_limit}；内存预算成立：{mem_needed}MB ≤ {mem_budget}MB"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
