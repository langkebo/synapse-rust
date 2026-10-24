#!/usr/bin/env python3
"""`updated_ts` 的约束形态必须二选一并冻结 —— 逐形态计数棘轮。

背景（2026-10-24 实测）
----------------------
项目规则 §2.2 把 `updated_ts` 定义成 **NULLABLE**，而基线 SQL 里两种写法各占一半：

| 形态 | 处数 | 语义 |
|---|---|---|
| `updated_ts BIGINT NOT NULL`（含 6 处 `DEFAULT clock_timestamp()`） | **43** | 每行必有更新时间 |
| `updated_ts BIGINT`（可空，含 1 处带 DEFAULT + 1 处 `ALTER … ADD COLUMN`） | **42** | "可能从没更新过" |

审计 ARCH-06 原文把它写成"44 处 NOT NULL 与规则冲突"，实际是**两套约定并存**，
谁也不是笔误。此时"统一成一种"需要数据迁移评估（可空改 NOT NULL 会在有 NULL 行时失败），
而**冻结现状 + 只许朝 NOT NULL 方向变**能立刻止住漂移，且成本为零。

判据
----
* `LOOSE`（可空）计数**增加** ⇒ 新表/新列又选了可空 ⇒ exit 1（要么改 NOT NULL，要么先改规则）；
* `STRICT`（NOT NULL）计数**减少** ⇒ 有人把已有列放宽了 ⇒ exit 1；
* 两个方向的"改善"（LOOSE 减少 / STRICT 增加）也必须跑 `--update` 收紧基线 ——
  单向棘轮，基线只能朝"更严格"走，防止它腐烂成"已知债务清单"。

用法
----
    python3 scripts/ci/check_schema_timestamp_convention.py            # 校验
    python3 scripts/ci/check_schema_timestamp_convention.py --print    # 列出当前两种形态的每一处
    python3 scripts/ci/check_schema_timestamp_convention.py --update   # 朝更严格方向收紧基线

为什么不是"直接把 42 处改成 NOT NULL"
------------------------------------
`ALTER TABLE … SET NOT NULL` 在任何一行 `updated_ts IS NULL` 时失败，而本仓没有生产数据样本
可用来判断这 42 张表是否真的都会有值。**规则改口 + 棘轮冻结**是当下唯一能证明"不再变坏"的做法；
真要做统一，得先有一轮数据普查（记在审计 ARCH-06 的"未覆盖"里）。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
SCHEMA = "migrations/00000000_unified_schema_v12.sql"
BASELINE = "scripts/ci/schema_timestamp_baseline"

# 一处 `updated_ts BIGINT` 的**任意**出现形态：`CREATE TABLE` 内联、单行建表、`ALTER … ADD COLUMN`
# 都算。刻意**不**用 `^\s*updated_ts` 锚定 —— 首版就是行首锚定，于是
# `CREATE TABLE probe (… , updated_ts BIGINT);` 这种单行建表整条漏过（变异实测没红）。
OCCURRENCE_RE = re.compile(r"\bupdated_ts\s+BIGINT(?P<rest>[^,)\n]*)", re.IGNORECASE)
ALTER_RE = re.compile(r"ALTER\s+TABLE\s+(?P<table>[A-Za-z0-9_.\"]+)", re.IGNORECASE)


def classify() -> tuple[list[tuple[int, str]], list[tuple[int, str]]]:
    """返回 (strict 行, loose 行)，元素是 (行号, 归一化后的片段)。"""
    strict: list[tuple[int, str]] = []
    loose: list[tuple[int, str]] = []
    for lineno, raw in enumerate(
        (REPO / SCHEMA).read_text(encoding="utf-8").splitlines(), 1
    ):
        if raw.lstrip().startswith("--"):
            continue
        alter = ALTER_RE.search(raw)
        for match in OCCURRENCE_RE.finditer(raw):
            rest = " ".join(match.group("rest").split())
            prefix = f"ALTER {alter.group('table')}: " if alter else ""
            entry = (lineno, f"{prefix}updated_ts BIGINT {rest}".strip())
            if "NOT NULL" in rest.upper():
                strict.append(entry)
            else:
                loose.append(entry)
    return strict, loose


def read_baseline() -> dict[str, int] | None:
    path = REPO / BASELINE
    if not path.exists():
        return None
    values: dict[str, int] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        key, _, value = line.partition("=")
        try:
            values[key.strip()] = int(value.strip())
        except ValueError:
            print(f"FAIL {BASELINE} 里 `{line}` 不是 KEY=VALUE 形式", file=sys.stderr)
            raise SystemExit(1) from None
    for required in ("STRICT", "LOOSE"):
        if required not in values:
            print(f"FAIL {BASELINE} 缺少 {required}", file=sys.stderr)
            raise SystemExit(1)
    return values


def write_baseline(strict: int, loose: int) -> None:
    (REPO / BASELINE).write_text(
        "# `updated_ts` 约束形态 —— 逐形态计数棘轮（见 scripts/ci/check_schema_timestamp_convention.py）。\n"
        "# 只允许朝更严格走：LOOSE 只能变小、STRICT 只能变大；改完跑 --update 收紧。\n"
        "# 规则口径见 .trae/rules/project_rules.md §2.2（两种写法都合法，新表必须显式选一个）。\n"
        "\n"
        f"STRICT={strict}\n"
        f"LOOSE={loose}\n",
        encoding="utf-8",
    )


def main(argv: list[str]) -> int:
    strict, loose = classify()

    if "--print" in argv:
        print(f"STRICT（NOT NULL）= {len(strict)}")
        for lineno, text in strict:
            print(f"  {SCHEMA}:{lineno}  {text}")
        print(f"LOOSE（可空）= {len(loose)}")
        for lineno, text in loose:
            print(f"  {SCHEMA}:{lineno}  {text}")
        return 0

    baseline = read_baseline()
    if baseline is None:
        if "--update" in argv:
            write_baseline(len(strict), len(loose))
            print(f"OK 已建立基线：STRICT={len(strict)} LOOSE={len(loose)}")
            return 0
        print(f"FAIL 缺少基线文件 {BASELINE}（先跑 --update 建立）", file=sys.stderr)
        return 1

    if "--update" in argv:
        if len(loose) > baseline["LOOSE"] or len(strict) < baseline["STRICT"]:
            print(
                "FAIL --update 只能朝更严格的方向收紧："
                f"当前 LOOSE={len(loose)}（基线 {baseline['LOOSE']}）、"
                f"STRICT={len(strict)}（基线 {baseline['STRICT']}）。\n"
                "放宽约束不是「更新基线」能批准的事 —— 要么改回 NOT NULL，要么在审计 ARCH-06 里"
                "单独论证。",
                file=sys.stderr,
            )
            return 1
        write_baseline(len(strict), len(loose))
        print(f"OK 基线已收紧：STRICT={len(strict)} LOOSE={len(loose)}")
        return 0

    problems: list[str] = []
    if len(loose) > baseline["LOOSE"]:
        problems.append(
            f"可空 `updated_ts` 从 {baseline['LOOSE']} 增到 {len(loose)} 处 —— 新表又选了可空形态。"
            " 请改成 `updated_ts BIGINT NOT NULL`（带 `DEFAULT` 更省事），或先在项目规则 §2.2 里改口径。"
            f"\n     定位：`python3 {Path(__file__).name} --print` 会逐条列出两种形态（含行号）。"
            "\n     注：这里**不猜**是哪一行新增的 —— 首版按列表下标取差集，结果在"
            "「数量不变但形态转移」时指到了无关行，比不指更误导。"
        )
    if len(strict) < baseline["STRICT"]:
        problems.append(
            f"NOT NULL 从 {baseline['STRICT']} 减到 {len(strict)} 处 —— 有人把已有列放宽了。"
            " 放宽约束会让「从没更新过」的行重新出现，`ORDER BY updated_ts` 的空值排序也随之不确定。"
        )

    if problems:
        print(
            "FAIL `updated_ts` 约束形态棘轮：\n  - " + "\n  - ".join(problems),
            file=sys.stderr,
        )
        return 1

    print(
        f"OK `updated_ts` 形态与基线一致（NOT NULL {len(strict)} / 可空 {len(loose)}）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
