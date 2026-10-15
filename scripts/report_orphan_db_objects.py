#!/usr/bin/env python3
"""孤儿数据库对象**只读**报告：真实库 vs 基线迁移。

## 为什么需要它

既有的 `check_schema_table_coverage.py` 只做**一个方向**：

    代码引用的表  ->  基线 migrations/ 里必须定义   （代码 -> 基线）

于是"**基线里没有、但老库里真实存在**"的对象**没有任何门禁看得见**。这类对象
2026-10-15 实测存在：`device_lists_outbound_pokes` / `room_state_events` /
`leak_alerts` / `push_devices`（均 0 行；前两者在代码里只剩注释），
以及 `idx_schema_migrations_version`（由 `docker/deploy/scripts/init-db.sql`、
`docker/db_migrate.sh`、`synapse-services/src/database_initializer/mod.rs` **三处**
分别创建，唯独基线里没有 ⇒ 新库与老库形态分叉）。

根因（**历史形态**）：基线 `00000000_unified_schema_v12.sql` 当时是 create-only + `IF NOT EXISTS`
（`DROP TABLE` 计数为 **0**），所以老库永久保留早期迁移链的残留，而全新库不会有 ——
**同一个版本号、两种库形态**。

2026-10-15 已把这条根因处理掉（口径收紧，不是删报告）：基线末尾新增「废弃表清理」段
（4 条 `DROP TABLE IF EXISTS`，方案 B5.3）把上面 4 张表折进去，`idx_schema_migrations_version`
则由「账本表口径收敛」段（方案 B5.4）的 `DROP INDEX IF EXISTS` 清掉 ⇒ 老库与全新库收敛。
本脚本的意义不变：**这类「基线没有、老库有」的漂移，除它之外没有别的门禁看得见**。
（注：现在基线里还有 3 处 `DROP TABLE _ck_probe_*`，那是 B4.5 的临时探针表在 `DO` 块内的自清理，
与「废弃表清理」是两回事。）

## 本脚本做什么、不做什么

* **只读**：只跑 `SELECT`（`pg_class` / `pg_index` / `pg_constraint` / `pg_depend`），
  **不执行任何 DDL/DML**。默认退出码 0（它是**报告**，不是门禁）。
* 报告五个方向：孤儿表/视图、未应用对象、孤儿索引（再按"是否挂在孤儿表上"分组）、
  孤儿**显式命名**约束、`NOT VALID`（未被验证）的 **CHECK** 约束。
  最后一项是 2026-10-15（方案 B4.4）补的：`NOT VALID` 只拦**新**写入、既有行不被校验
  —— 也就是"`ADD … NOT VALID` 忘了配对 `VALIDATE`"的典型形态；而**模板路线的测试看不到它**
  （`LIKE … INCLUDING ALL` 克隆会把 CHECK 重建为已验证），所以只能在真实库上查。
* 三类刻意的假阳性规避（都是实测踩出来的，别删）：
  1. **扩展自带对象**（`pg_stat_statements` / `pg_stat_statements_info`）——
     靠 `pg_depend.deptype = 'e'` 排除，否则装了扩展就一片红；
  2. **由 PK/UNIQUE 隐式创建的索引** —— 靠 `pg_constraint.conindid` 判定并排除，
     否则每条 `PRIMARY KEY` 都会假报成孤儿索引；
  3. **自动命名的约束**（`x_pkey` / `x_key` / `x_fkey` / `x_col_check`）——
     基线里大量 PRIMARY KEY 是**内联无名**写的，PostgreSQL 会自动命名，直接比对必假报。
* 基线侧解析同时覆盖**内联** `CONSTRAINT <name> ...`（实测 390 处）与
  `ALTER TABLE ... ADD CONSTRAINT <name>`（35 处），并跳过 `DROP CONSTRAINT IF EXISTS`
  里的 `IF`（否则会把 `if` 当成约束名）。

## 用法

    # 默认走本机容器 synapse-postgres（dev 与 deploy 栈的容器名相同）
    python3 scripts/report_orphan_db_objects.py

    # 换容器/账号/库
    python3 scripts/report_orphan_db_objects.py --container synapse-postgres \
        --user synapse --db synapse

    # 容器不可用时用连接串直连（需要本机 psql）
    DATABASE_URL=postgresql://synapse@127.0.0.1:5432/synapse \
        python3 scripts/report_orphan_db_objects.py --dsn-env DATABASE_URL

    # 机器可读 + 巡检模式（有结构性漂移即 exit 1）
    python3 scripts/report_orphan_db_objects.py --json-report - --strict

配套运维说明见 `docs/runbook/DB_ORPHAN_OBJECT_AUDIT.md`。
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = ROOT / "migrations"

# ── 基线侧解析 ────────────────────────────────────────────────────────────────
_BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
_LINE_COMMENT = re.compile(r"--[^\n]*")

DEF_RE = {
    "table": re.compile(
        r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-z_][a-z0-9_]*)", re.IGNORECASE
    ),
    "view": re.compile(
        r"CREATE\s+(?:OR\s+REPLACE\s+)?(?:MATERIALIZED\s+)?VIEW"
        r"(?:\s+IF\s+NOT\s+EXISTS)?\s+([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    ),
    "index": re.compile(
        r"CREATE\s+(?:UNIQUE\s+)?INDEX(?:\s+CONCURRENTLY)?(?:\s+IF\s+NOT\s+EXISTS)?\s+"
        r"([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    ),
    # 同时覆盖内联 `CONSTRAINT name ...` 与 `ADD CONSTRAINT name`。三个否定条件都是
    # 实测踩出来的：`(?<![a-z_])` 避开 `pg_constraint`；`(?!IF\b)` 避开
    # `DROP CONSTRAINT IF EXISTS`；`(?![\w%])` 避开生成式模板里的 `ck_%I`。
    "constraint": re.compile(
        r"(?<![a-z_])CONSTRAINT\s+(?!IF\b)([a-z_][a-z0-9_]*)(?![\w%])", re.IGNORECASE
    ),
}

# PostgreSQL 对"未显式命名"的约束生成的默认名后缀（`x_pkey` / `x_key` / `x_fkey` /
# `x_col_check`）。基线里大量 PRIMARY KEY / UNIQUE 是**内联无名**写的，若不做这层
# 过滤，每条都会假报成孤儿约束。
_AUTO_CONSTRAINT_SUFFIX = re.compile(r"_(pkey|key|fkey|check|excl)$", re.IGNORECASE)

# 扩展自带的对象（`pg_stat_statements` 等）不是孤儿：它们由 `CREATE EXTENSION` 产生。
# 用统一的谓词排除，别在 Python 侧硬编码扩展对象名。
_NOT_EXTENSION_MEMBER = "NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = {alias}.oid AND d.deptype = 'e')"


# 基线里用 `%I` **动态拼名字**的约束模板（例如
# `ALTER TABLE %I ADD CONSTRAINT ck_%I_user_id_format ...`）。
# 这类约束**确实由基线创建**，只是没有任何静态名单 —— 不做这层识别，
# 报告会把 114 条正常约束报成孤儿（2026-10-15 实测：106 条 `*_user_id_format`
# + 8 条 `*_mxc_format`）。模板从基线**自动派生**，不硬编码名字。
_GENERATED_CONSTRAINT_RE = re.compile(
    r"ADD\s+CONSTRAINT\s+([a-z_]+)%I([a-z_]+)", re.IGNORECASE
)


def _generated_name_patterns(text: str) -> list[re.Pattern[str]]:
    """把 `ADD CONSTRAINT <prefix>%I<suffix>` 模板编译成名字正则。"""
    return [
        re.compile("^" + re.escape(p.lower()) + ".+" + re.escape(s.lower()) + "$")
        for p, s in _GENERATED_CONSTRAINT_RE.findall(text)
    ]


def is_generated_name(name: str, patterns: list[re.Pattern[str]]) -> bool:
    return any(p.match(name.lower()) for p in patterns)


# `VALIDATE CONSTRAINT <name>` **只引用**已有约束，并不创建它。若把它算作"基线定义"，
# 就会掩盖一类静默失效：`IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = '<旧名>')`
# 守卫 + VALIDATE 旧名 ⇒ 该分支**永不触发**（实测基线里就有 1 处：创建的是
# `fk_cross_signing_keys_user_id`，却在校验 `fk_cross_signing_keys_user`）。
# 故先摘掉 VALIDATE 子句再解析"定义"，并把这类引用单列报告。
_VALIDATE_RE = re.compile(r"VALIDATE\s+CONSTRAINT\s+([a-z_][a-z0-9_]*)", re.IGNORECASE)

# 已知**有意**为 `NOT VALID`、且基线**不打算** `VALIDATE` 的 CHECK 约束（按约束名）。
#
# 依据 `migrations/00000000_unified_schema_v12.sql` 的 MSC4354 sticky 段：那两条约束是在
# 同一块刚 `ADD COLUMN expires_at` 之后加的，存量行两列同为 `NULL` ⇒ 必然满足；作者选
# `NOT VALID` 只为**省掉大表上的一次全量扫描**（基线原注释就写了这条理由）。
#
# ⚠️ 往这里加条目必须写清"为什么不校验"。默认口径是：`NOT VALID` 的 CHECK 就是**缺口**
# —— 它只约束新写入，历史数据不被校验，等于约束对存量行形同虚设。
_UNVALIDATED_CHECK_ALLOWLIST = {
    "ck_room_sticky_events_expiry_pair",
    "ck_room_sticky_events_duration_positive",
}


def read_baseline(
    path: pathlib.Path,
) -> tuple[dict[str, set[str]], list[re.Pattern[str]], set[str]]:
    """解析基线：对象名集合 + 动态生成的名字模板 + VALIDATE 引用的名字。"""
    defs: dict[str, set[str]] = {kind: set() for kind in DEF_RE}
    generated: list[re.Pattern[str]] = []
    validated: set[str] = set()
    files = sorted(path.rglob("*.sql")) if path.is_dir() else [path]
    for sql_file in files:
        text = sql_file.read_text(encoding="utf-8", errors="replace")
        text = _BLOCK_COMMENT.sub(" ", text)
        text = _LINE_COMMENT.sub(" ", text)
        validated.update(m.lower() for m in _VALIDATE_RE.findall(text))
        text = _VALIDATE_RE.sub(" ", text)
        for kind, pattern in DEF_RE.items():
            defs[kind].update(m.group(1).lower() for m in pattern.finditer(text))
        generated.extend(_generated_name_patterns(text))
    return defs, generated, validated


# ── 实时库侧查询（全部只读）─────────────────────────────────────────────────
LIVE_QUERIES = {
    # relkind: r=普通表 p=分区表 m=物化视图 v=视图（排除 S=序列、i=索引）
    "relation": (
        "SELECT c.relname || '|' || c.relkind::text FROM pg_class c "
        "JOIN pg_namespace n ON n.oid = c.relnamespace "
        "WHERE n.nspname = %(schema)s AND c.relkind IN ('r', 'p', 'v', 'm') "
        "AND " + _NOT_EXTENSION_MEMBER.format(alias="c") + " ORDER BY 1"
    ),
    # 输出 索引名|是否由约束承载|所属表名：
    #   第 2 列排除 PK/UNIQUE 隐式建的索引；第 3 列把"挂在孤儿表上的索引"单独分组
    #   （它们会随表一起 DROP，不必单独处置）。
    "index": (
        "SELECT i.relname || '|' || (con.conname IS NOT NULL)::text || '|' || t.relname "
        "FROM pg_index x "
        "JOIN pg_class i ON i.oid = x.indexrelid "
        "JOIN pg_class t ON t.oid = x.indrelid "
        "JOIN pg_namespace n ON n.oid = t.relnamespace "
        "LEFT JOIN pg_constraint con ON con.conindid = x.indexrelid "
        "WHERE n.nspname = %(schema)s AND t.relkind IN ('r', 'p', 'm') "
        "AND " + _NOT_EXTENSION_MEMBER.format(alias="i") + " ORDER BY 1"
    ),
    # contype: t=constraint trigger（排除）。同时输出所属表名，便于把
    # "挂在孤儿表上的约束"单独分组（它们会随表一起 DROP）。
    "constraint": (
        "SELECT con.conname || '|' || t.relname FROM pg_constraint con "
        "JOIN pg_namespace n ON n.oid = con.connamespace "
        "JOIN pg_class t ON t.oid = con.conrelid "
        "WHERE n.nspname = %(schema)s AND con.contype <> 't' "
        "AND " + _NOT_EXTENSION_MEMBER.format(alias="t") + " ORDER BY 1"
    ),
    # `convalidated = false` 的 **CHECK** 约束（即 `NOT VALID`）。
    # 只取 `contype = 'c'` 是**有意的**：基线里另有十几条 `NOT VALID` 的**外键**
    # （`CREATE TABLE` 内联声明 —— 新建表本就无行可验，风险为零），混进来只会淹没信号。
    # CHECK 的 `NOT VALID` 才是真实缺口：只拦新写入、历史行不被校验。
    "unvalidated_check": (
        "SELECT con.conname || '|' || t.relname FROM pg_constraint con "
        "JOIN pg_namespace n ON n.oid = con.connamespace "
        "JOIN pg_class t ON t.oid = con.conrelid "
        "WHERE n.nspname = %(schema)s AND con.contype = 'c' AND NOT con.convalidated "
        "AND " + _NOT_EXTENSION_MEMBER.format(alias="t") + " ORDER BY 1"
    ),
}

RELKIND_LABEL = {"r": "table", "p": "partitioned table", "v": "view", "m": "matview"}


class ReportError(Exception):
    """一条可读的失败原因。"""


def psql_prefix(args: argparse.Namespace) -> list[str]:
    """构造 psql 调用前缀（容器内优先，退化为本机 psql）。"""
    if args.dsn_env:
        dsn = os.environ.get(args.dsn_env)
        if not dsn:
            raise ReportError(f"环境变量 {args.dsn_env} 为空，无法连接数据库")
        if not shutil.which("psql"):
            raise ReportError("本机没有 psql，无法使用 --dsn-env；请改用容器模式")
        return ["psql", dsn]
    if not shutil.which("docker"):
        raise ReportError("本机没有 docker，请用 --dsn-env 指定连接串")
    return [
        "docker",
        "exec",
        "-i",
        args.container,
        "psql",
        "-U",
        args.user,
        "-d",
        args.db,
    ]


def _render(sql: str, schema: str) -> str:
    """把 %(schema)s 换成字面量（psql 不做参数绑定；schema 名先做白名单校验）。"""
    if not re.fullmatch(r"[a-z_][a-z0-9_]*", schema):
        raise ReportError(f"schema 名不合法：{schema!r}")
    return sql.replace("%(schema)s", f"'{schema}'")


def run_query(prefix: list[str], sql: str, schema: str) -> list[str]:
    """跑一条只读查询，返回非空行（已 strip）。"""
    cmd = [*prefix, "-v", "ON_ERROR_STOP=1", "-tAc", _render(sql, schema)]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        detail = (proc.stderr or proc.stdout).strip().splitlines()
        raise ReportError(
            "查询失败（容器/账号/库名是否正确？）：\n  "
            + "\n  ".join(detail[:3] or ["<无错误输出>"])
        )
    return [line.strip() for line in proc.stdout.splitlines() if line.strip()]


def collect_live(prefix: list[str], args: argparse.Namespace) -> dict:
    """采集实时库对象。"""
    live: dict = {
        "table": set(),
        "view": set(),
        "index": set(),
        "constraint": set(),
        "constraint_backed_index": set(),
        "unvalidated_check": set(),
    }
    index_owner: dict[str, str] = {}
    constraint_owner: dict[str, str] = {}

    for line in run_query(prefix, LIVE_QUERIES["relation"], args.schema):
        name, _, relkind = line.rpartition("|")
        kind = RELKIND_LABEL.get(relkind)
        if kind in ("table", "partitioned table"):
            live["table"].add(name.lower())
        elif kind:  # view / matview
            live["view"].add(name.lower())

    for line in run_query(prefix, LIVE_QUERIES["index"], args.schema):
        # 查询输出固定三段：索引名|是否由约束承载|所属表名（三者都不含 '|'）
        name, backed, owner = line.split("|", 2)
        if backed.strip().lower() == "true":
            live["constraint_backed_index"].add(name.lower())
        else:
            live["index"].add(name.lower())
            index_owner[name.lower()] = owner.lower()

    for line in run_query(prefix, LIVE_QUERIES["constraint"], args.schema):
        # 固定两段：约束名|所属表名
        name, _, table = line.rpartition("|")
        live["constraint"].add(name.lower())
        constraint_owner[name.lower()] = table.lower()

    for line in run_query(prefix, LIVE_QUERIES["unvalidated_check"], args.schema):
        # 固定两段：约束名|所属表名（表名只用于人工核对，分组不需要）
        name, _, _table = line.rpartition("|")
        live["unvalidated_check"].add(name.lower())

    live["_index_owner"] = index_owner
    live["_constraint_owner"] = constraint_owner
    return live


def build_report(
    defs: dict[str, set[str]],
    live: dict,
    generated: list[re.Pattern[str]],
    validated: set[str],
) -> dict:
    """算出各方向的差异。"""

    def named_constraints(names: set[str]) -> set[str]:
        """过滤掉自动命名的与基线动态生成的（两者都不是孤儿）。"""
        return {
            name
            for name in names
            if not _AUTO_CONSTRAINT_SUFFIX.search(name)
            and not is_generated_name(name, generated)
        }

    live_relations = live["table"] | live["view"]
    base_relations = defs["table"] | defs["view"]
    base_named_index = defs["index"]
    base_named_constraint = named_constraints(defs["constraint"])
    live_named_constraint = named_constraints(live["constraint"])

    orphan_relations = live_relations - base_relations
    orphan_indexes = live["index"] - base_named_index
    orphan_constraints = live_named_constraint - base_named_constraint
    index_owner: dict[str, str] = live["_index_owner"]
    constraint_owner: dict[str, str] = live["_constraint_owner"]
    # `NOT VALID` 的 CHECK：扣掉白名单里"有意为之"的那几条，剩下的都是缺口
    live_unvalidated_check = live["unvalidated_check"]
    unvalidated_checks = live_unvalidated_check - _UNVALIDATED_CHECK_ALLOWLIST
    return {
        "orphan_relations": sorted(orphan_relations),
        "missing_relations": sorted(base_relations - live_relations),
        # 挂在孤儿表上的索引/约束会随表一起 DROP，单独分组避免误导处置顺序
        "orphan_indexes_on_orphan_tables": sorted(
            idx for idx in orphan_indexes if index_owner.get(idx) in orphan_relations
        ),
        "orphan_indexes_on_live_tables": sorted(
            idx
            for idx in orphan_indexes
            if index_owner.get(idx) not in orphan_relations
        ),
        "orphan_constraints_on_orphan_tables": sorted(
            con
            for con in orphan_constraints
            if constraint_owner.get(con) in orphan_relations
        ),
        "orphan_constraints_on_live_tables": sorted(
            con
            for con in orphan_constraints
            if constraint_owner.get(con) not in orphan_relations
        ),
        "missing_constraints": sorted(base_named_constraint - live_named_constraint),
        # VALIDATE 引用了、但基线从未创建的名字 ⇒ 对应守卫分支永不触发
        "dead_validate_references": sorted(validated - defs["constraint"]),
        # `NOT VALID` 的 CHECK（已扣除白名单）：只拦新写入、历史行不被校验
        "unvalidated_checks": sorted(unvalidated_checks),
        "unvalidated_checks_allowlisted": sorted(
            live_unvalidated_check & _UNVALIDATED_CHECK_ALLOWLIST
        ),
        "counts": {
            "baseline_relations": len(base_relations),
            "live_relations": len(live_relations),
            "baseline_named_indexes": len(base_named_index),
            "live_indexes": len(live["index"]),
            "live_constraint_backed_indexes": len(live["constraint_backed_index"]),
            "baseline_named_constraints": len(base_named_constraint),
            "live_named_constraints": len(live_named_constraint),
            "live_unvalidated_checks": len(live_unvalidated_check),
            "generated_name_templates": len(generated),
        },
    }


def print_report(
    report: dict, baseline: pathlib.Path, args: argparse.Namespace
) -> None:
    counts = report["counts"]
    print(f"基线：{baseline}（关系 {counts['baseline_relations']} 个）")
    print(
        f"实时库：{args.schema}（关系 {counts['live_relations']} 个；"
        f"非约束索引 {counts['live_indexes']} 个，另有 "
        f"{counts['live_constraint_backed_indexes']} 个由 PK/UNIQUE 承载已排除；"
        f"显式命名约束 {counts['live_named_constraints']} 个）"
    )
    print()

    def section(title: str, items: list[str], hint: str = "") -> None:
        marker = "⚠️" if items else "✅"
        print(f"{marker} {title}：{len(items)}")
        for item in items:
            print(f"    - {item}")
        if items and hint:
            print(f"    → {hint}")

    section(
        "库里有、基线没有的表/视图（孤儿）",
        report["orphan_relations"],
        "老库残留，或由引导脚本/手工创建。处置前先逐表 `pg_dump -t <表>` 存证，"
        "再把 `DROP TABLE IF EXISTS` 折入基线（否则新库/老库永久分叉）。",
    )
    section(
        "基线有、库里没有的表/视图（未应用）",
        report["missing_relations"],
        "迁移没跑完，或被人手工删过 —— 两种情况都会让「同一版本号、两种库形态」。",
    )
    section(
        "孤儿索引 · 挂在孤儿表上（随表一起处理，不必单独 DROP）",
        report["orphan_indexes_on_orphan_tables"],
    )
    section(
        "孤儿索引 · 挂在存活表上（需单独处置）",
        report["orphan_indexes_on_live_tables"],
        "手工建的索引不会被迁移链复现，换机器就消失（性能回退难以归因）。"
        "若来自引导脚本（如 init-db.sql），应折入基线使其成为单一来源。",
    )
    section(
        "孤儿约束 · 挂在孤儿表上（随表一起处理）",
        report["orphan_constraints_on_orphan_tables"],
    )
    section(
        "孤儿约束 · 挂在存活表上（需单独处置）",
        report["orphan_constraints_on_live_tables"],
        "这是真正需要动手的一类。注意口径：`_pkey`/`_key`/`_fkey`/`_check` 结尾的"
        "按自动命名过滤；基线用 `%I` 动态生成的（见 counts.generated_name_templates）也已排除。",
    )
    section(
        "基线有、库里没有的显式命名约束（未应用）",
        report["missing_constraints"],
    )
    section(
        "基线内 `VALIDATE CONSTRAINT` 引用了基线**从未创建**的名字（死分支）",
        report["dead_validate_references"],
        "写法是 `IF EXISTS (... conname = '<旧名>') THEN VALIDATE CONSTRAINT <旧名>` ⇒ "
        "守卫永不成立、校验被静默跳过。若该约束由别处创建且本就 VALIDATED，则无实际影响，"
        "但仍属**陈旧引用**，应把名字对齐。",
    )
    section(
        "`NOT VALID`（未被验证）的 CHECK 约束 —— 只拦新写入，历史行不被校验",
        report["unvalidated_checks"],
        "规范写法是 `ADD CONSTRAINT … NOT VALID` 之后**在同一份基线里**独立 "
        "`ALTER TABLE … VALIDATE CONSTRAINT …`（方案 B4.4）。⚠️ 模板路线的测试**看不到**这种遗漏"
        "（`LIKE … INCLUDING ALL` 克隆会把 CHECK 重建为已验证），只有真实库才看得见。",
    )
    if report["unvalidated_checks_allowlisted"]:
        print(
            "（另有已知**有意**为之、不计入上项的："
            + "、".join(report["unvalidated_checks_allowlisted"])
            + "；理由见脚本顶部 `_UNVALIDATED_CHECK_ALLOWLIST`）"
        )
    print()
    if report["orphan_relations"] or report["missing_relations"]:
        print("结论：存在**结构性漂移**（不只是多几个索引）。")
    else:
        print("结论：表/视图与基线一致（索引/约束差异见上）。")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="孤儿数据库对象只读报告（真实库 vs 基线迁移）",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--baseline", type=pathlib.Path, default=DEFAULT_BASELINE)
    parser.add_argument("--container", default="synapse-postgres")
    parser.add_argument("--user", default="synapse")
    parser.add_argument("--db", default="synapse")
    parser.add_argument("--schema", default="public")
    parser.add_argument(
        "--dsn-env",
        default=None,
        help="用环境变量名里的连接串直连（需要本机 psql）；不指定则走 docker exec",
    )
    parser.add_argument(
        "--json-report",
        nargs="?",
        const="-",
        help="输出 JSON（不带值 = stdout；带路径 = 写文件）",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help=(
            "存在孤儿表/视图、未应用对象、或未被验证（NOT VALID）的 CHECK 约束时以 1 退出"
            "（供巡检使用；白名单内的有意项不计）"
        ),
    )
    args = parser.parse_args()

    if not args.baseline.exists():
        print(f"FATAL: 基线路径不存在：{args.baseline}", file=sys.stderr)
        return 2

    defs, generated, validated = read_baseline(args.baseline)
    try:
        live = collect_live(psql_prefix(args), args)
    except ReportError as exc:
        print(f"FATAL: {exc}", file=sys.stderr)
        return 2

    report = build_report(defs, live, generated, validated)
    if args.json_report is not None:
        payload = {
            "tool": "report_orphan_db_objects",
            "baseline": str(args.baseline),
            "target": {
                "container": None if args.dsn_env else args.container,
                "db": args.db,
                "schema": args.schema,
            },
            **report,
        }
        text = json.dumps(payload, ensure_ascii=False, indent=2) + "\n"
        if args.json_report == "-":
            print(text)
        else:
            out = pathlib.Path(args.json_report)
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(text, encoding="utf-8")
            print(f"JSON 报告已写入 {out}")
    else:
        print_report(report, args.baseline, args)

    if args.strict and (
        report["orphan_relations"]
        or report["missing_relations"]
        or report["unvalidated_checks"]
    ):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
