#!/usr/bin/env python3
r"""`ORDER BY <毫秒时间戳>` 必须带决胜键 —— 逐文件计数棘轮。

背景（2026-09-22 实测）
----------------------
`refresh_token::get_rotations` 曾是 `ORDER BY rotated_ts DESC`（`rotated_ts` 是 BIGINT
**毫秒**），同一毫秒内两次轮换并列时 PostgreSQL 可任意顺序返回 —— "最近的在前"这条契约
在并列时不确定。小表顺序扫描下通常返回插入顺序，于是最旧的排在前面；按 CI 口径跑覆盖率
时 `test_db_record_rotation_and_get_rotations` 就是这么失败的（`left: "new_hash_1"`）。

时间戳（毫秒/秒）不是唯一键，按它排序**必须**再带一个唯一/单调的决胜键
（`, id DESC`、`, event_id DESC`、`, media_id DESC` …），否则顺序不确定。

判据
----
扫 `src/` 与各 workspace crate 的 `src/`（含内联 `#[cfg(test)]` 夹具），找
`ORDER BY <x>_ts [ASC|DESC]` 且**只有这一个键**的站点；已带第二键（如
`, stream_ordering ASC`）的不计。逐文件计数与基线比较：

* 某文件计数**增加** ⇒ 新增了不定的排序 ⇒ exit 1；
* 某文件计数**减少** ⇒ 修好了/删掉了 ⇒ exit 1，要求跑 `--update` 收紧基线
  （单向棘轮：基线只允许变短，防止它腐烂成"已知债务清单"）。

用法
----
    python3 scripts/ci/check_ts_order_tiebreak.py            # 校验
    python3 scripts/ci/check_ts_order_tiebreak.py --print    # 只打印当前站点
    python3 scripts/ci/check_ts_order_tiebreak.py --update   # 收紧基线

已知高价值残留（有意留白，需要单独一轮 + 事件分页的 keyset 语义评审）：
`synapse-storage/src/event/{basic,batch,dag,pagination,redaction,search,state}.rs`
按 `origin_server_ts` 排序的 20+ 处 —— Matrix 事件排序的正解是第二键
`stream_ordering`（部分站点已经这么写了）。
"""

import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
BASELINE = ROOT / "scripts" / "ci" / "ts_order_single_key_baseline"

SCAN_DIRS = [
    "src",
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "synapse-storage/src",
    "synapse-web/src",
]

ORDER_BY = re.compile(r"ORDER\s+BY\s+(.+)", re.IGNORECASE)
TS_KEY = re.compile(r"^[A-Za-z_][A-Za-z0-9_.]*_ts$", re.IGNORECASE)
DIRECTION = re.compile(r"\s+(DESC|ASC)(\s+NULLS\s+(FIRST|LAST))?\s*$", re.IGNORECASE)


def rust_files() -> list[Path]:
    out = subprocess.run(
        [
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    files = []
    for rel in out.split("\0"):
        if not rel:
            continue
        norm = rel.replace("\\", "/")
        if any(norm == d or norm.startswith(d + "/") for d in SCAN_DIRS):
            files.append(ROOT / norm)
    return files


def single_key_locations(text: str) -> list[int]:
    """返回 1-based 行号：该行的 ORDER BY 只有**一个** `*_ts` 键。"""
    hits = []
    for lineno, line in enumerate(text.splitlines(), 1):
        # 跳过 Rust 注释行：正文里解释"这里是 ORDER BY created_ts DESC"很常见，
        # 把散文当 SQL 计数会让基线虚高，还会让"新增一句注释"变成假红
        # （同类教训：SQLx 计数器的注释缺陷）。
        stripped = line.lstrip()
        if (
            stripped.startswith("//")
            or stripped.startswith("*")
            or stripped.startswith("#")
        ):
            continue
        m = ORDER_BY.search(line)
        if not m:
            continue
        clause = m.group(1)
        # 截断到子句边界：字符串结束、续行反斜杠、LIMIT/OFFSET/RETURNING/分号/右括号。
        for stop in ('"#', '"', "\\", ";", ")"):
            idx = clause.find(stop)
            if idx != -1:
                clause = clause[:idx]
        clause = re.split(
            r"\b(LIMIT|OFFSET|RETURNING|FOR\s+UPDATE)\b", clause, maxsplit=1, flags=re.I
        )[0]
        clause = clause.strip()
        if not clause:
            continue
        # 以逗号结尾 ⇒ 键在下一行继续，视为多键（避免把跨行多键误判成单键）。
        if clause.endswith(","):
            continue
        keys = [k.strip() for k in clause.split(",") if k.strip()]
        if len(keys) != 1:
            continue
        first = DIRECTION.sub("", keys[0]).strip()
        if TS_KEY.match(first):
            hits.append(lineno)
    return hits


def scan() -> tuple[Counter, list[tuple[str, int, str]]]:
    counts: Counter = Counter()
    sites: list[tuple[str, int, str]] = []
    for path in rust_files():
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for lineno in single_key_locations(text):
            rel = path.relative_to(ROOT).as_posix()
            counts[rel] += 1
            sites.append((rel, lineno, text.splitlines()[lineno - 1].strip()))
    return counts, sites


def read_baseline() -> Counter:
    if not BASELINE.exists():
        return Counter()
    counts: Counter = Counter()
    for raw in BASELINE.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        path, _, value = line.rpartition(" ")
        counts[path.strip()] = int(value)
    return counts


def write_baseline(counts: Counter) -> None:
    lines = [
        "# `ORDER BY <x>_ts` 单键站点 —— 逐文件计数棘轮（见 scripts/ci/check_ts_order_tiebreak.py）。",
        "# 只允许变短：修好一处后跑 --update 收紧；新增一处会红。",
        "# 高价值残留：synapse-storage/src/event/* 按 origin_server_ts 排序需补 stream_ordering。",
        "",
    ]
    for path in sorted(counts):
        lines.append(f"{path} {counts[path]}")
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    counts, sites = scan()
    if "--print" in sys.argv:
        for path, lineno, text in sites:
            print(f"{path}:{lineno}  {text[:110]}")
        print(f"\n单键站点合计：{sum(counts.values())} 处 / {len(counts)} 个文件")
        return 0
    if "--update" in sys.argv:
        write_baseline(counts)
        print(
            f"baseline rewritten: {sum(counts.values())} single-key sites -> {BASELINE}"
        )
        return 0

    baseline = read_baseline()
    if not baseline:
        print(
            f"FAIL: baseline {BASELINE} is missing or empty (run --update)",
            file=sys.stderr,
        )
        return 2

    grown = {
        p: (baseline.get(p, 0), counts[p])
        for p in counts
        if counts[p] > baseline.get(p, 0)
    }
    shrank = {
        p: (baseline[p], counts.get(p, 0))
        for p in baseline
        if counts.get(p, 0) < baseline[p]
    }
    if grown:
        print(
            "FAIL 新增了不带决胜键的 `ORDER BY <*_ts>`（并列时顺序不确定）",
            file=sys.stderr,
        )
        for path, (was, now) in sorted(grown.items()):
            print(f"  {path}: {was} -> {now}", file=sys.stderr)
            for p, lineno, text in sites:
                if p == path:
                    print(f"      {p}:{lineno}  {text[:100]}", file=sys.stderr)
        print(
            "  修法：加第二排序键（`, id DESC` / `, stream_ordering ASC` / `, event_id DESC` …）",
            file=sys.stderr,
        )
        return 1
    if shrank:
        print(
            "FAIL 基线已过期（这些站点的单键排序已修好或删除，必须收紧基线）",
            file=sys.stderr,
        )
        for path, (was, now) in sorted(shrank.items()):
            print(f"  {path}: {was} -> {now}", file=sys.stderr)
        print(
            "  执行：python3 scripts/ci/check_ts_order_tiebreak.py --update",
            file=sys.stderr,
        )
        return 1

    print(
        f"OK: `ORDER BY <*_ts>` 单键站点与基线一致（{sum(counts.values())} 处 / {len(counts)} 个文件）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
