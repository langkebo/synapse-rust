#!/usr/bin/env python3
"""`sleep` 站点计数棘轮：只许下降，且必须分桶。

背景（2026-10-24 实测）
----------------------
审计 TEST-05 原记"27 处 / 14 文件"。实测（`tokio::time::sleep` + `thread::sleep`）混着三种
完全不同的东西，合成一个数字会让"删掉一个测试 sleep、新增一个生产 sleep"看起来是净改善：

| 桶 | 含义 | 为什么不能一视同仁 |
|---|---|---|
| `PROD` | 非测试文件、且不在 `#[cfg(test)]` 区里 | 多为重试退避 / 长轮询等待，**有正当理由**；但也是"启动时睡一下等对端"的藏身处 |
| `TEST_INLINE` | 生产文件里的 `#[cfg(test)] mod` 内 | 纯 flake 面：`sleep(200ms)` 等后台任务跑完，机器慢就红 |
| `TEST_FILE` | `tests/**` 与 `*_tests.rs` | 同上 |

判据
----
* 任一分桶**增加** ⇒ exit 1；
* 减少 ⇒ 也要 `--update` 收紧基线（单向棘轮，基线只许变小）。

用法
----
    python3 scripts/ci/check_sleep_ratchet.py            # 校验
    python3 scripts/ci/check_sleep_ratchet.py --print    # 逐桶逐文件列出
    python3 scripts/ci/check_sleep_ratchet.py --update   # 收紧基线（只能变小）

为什么不自动判定"是等事件还是等固定窗口"
--------------------------------------
试过：`loop` / `select!` / `timeout` 的邻近关系判不出意图 —— `sleep(100ms)` 既可能是
"等后台任务"（flake）也可能是"空闲轮询"（正当）。自动分桶会得出"已治理"的假象。
所以本门禁只做**总量不增长**这件能被证明的事；逐处意图分类是审计 TEST-05 的表格，不是判据。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
BASELINE = "scripts/ci/sleep_baseline"

SLEEP_RE = re.compile(
    r"tokio::time::sleep\s*\(|\bthread::sleep\s*\(|\bsleep\s*\(\s*(?:tokio::time::)?Duration"
)
CFG_TEST_RE = re.compile(r"^\s*#\[cfg\(test\)\]")


def tracked_rust_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*.rs"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def is_test_file(rel: str) -> bool:
    return (
        rel.startswith("tests/")
        or "/tests/" in rel
        or rel.endswith("_tests.rs")
        or rel.endswith("/tests.rs")
    )


def cfg_test_spans(lines: list[str]) -> list[tuple[int, int]]:
    """返回 `#[cfg(test)] mod … { … }` 的行号区间（1-based，闭区间）。"""
    spans: list[tuple[int, int]] = []
    for idx, line in enumerate(lines):
        if not CFG_TEST_RE.match(line):
            continue
        for probe in range(idx + 1, min(idx + 4, len(lines))):
            if re.match(r"\s*(?:pub\s+)?mod\s+\w+", lines[probe]):
                depth = 0
                started = False
                for cursor in range(probe, len(lines)):
                    depth += lines[cursor].count("{") - lines[cursor].count("}")
                    if "{" in lines[cursor]:
                        started = True
                    if started and depth <= 0:
                        spans.append((idx + 1, cursor + 1))
                        break
                break
    return spans


def collect() -> dict[str, dict[str, int]]:
    """返回 {bucket: {file: count}}。"""
    buckets: dict[str, dict[str, int]] = {
        "PROD": {},
        "TEST_INLINE": {},
        "TEST_FILE": {},
    }
    for rel in tracked_rust_files():
        try:
            lines = (
                (REPO / rel).read_text(encoding="utf-8", errors="replace").splitlines()
            )
        except OSError:
            continue
        spans = [] if is_test_file(rel) else cfg_test_spans(lines)
        for lineno, line in enumerate(lines, 1):
            if line.lstrip().startswith("//"):
                continue
            if not SLEEP_RE.search(line):
                continue
            if is_test_file(rel):
                bucket = "TEST_FILE"
            elif any(start <= lineno <= end for start, end in spans):
                bucket = "TEST_INLINE"
            else:
                bucket = "PROD"
            buckets[bucket][rel] = buckets[bucket].get(rel, 0) + 1
    return buckets


def totals(buckets: dict[str, dict[str, int]]) -> dict[str, int]:
    return {bucket: sum(files.values()) for bucket, files in buckets.items()}


def read_baseline() -> dict[str, int]:
    path = REPO / BASELINE
    values: dict[str, int] = {}
    if not path.exists():
        return values
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        # 明细行（`PROD_FILE <path> <n>`）只给人看，不参与比较 —— 首版把它当 KEY=VALUE
        # 解析，于是 `--update` 写完基线、下一次校验立刻红。
        if line.startswith("PROD_FILE "):
            continue
        key, _, value = line.partition("=")
        try:
            values[key.strip()] = int(value.strip())
        except ValueError:
            print(f"FAIL {BASELINE}: `{line}` 不是 KEY=VALUE", file=sys.stderr)
            raise SystemExit(1) from None
    return values


def write_baseline(current: dict[str, int], buckets: dict[str, dict[str, int]]) -> None:
    lines = [
        "# `sleep` 站点计数棘轮（见 scripts/ci/check_sleep_ratchet.py）。",
        "# 只允许减少：删掉一处后跑 --update 收紧；新增一处会红（先说明它为什么不是 flake 源）。",
        "# 逐桶：PROD=生产代码 / TEST_INLINE=生产文件里的 #[cfg(test)] / TEST_FILE=tests/**。",
        "",
    ]
    lines += [
        f"{bucket}={current[bucket]}" for bucket in ("PROD", "TEST_INLINE", "TEST_FILE")
    ]
    lines.append("")
    lines.append("# 逐文件明细（PROD 桶；新增文件会在这里体现出来）")
    for rel, count in sorted(buckets["PROD"].items()):
        lines.append(f"PROD_FILE {rel} {count}")
    (REPO / BASELINE).write_text("\n".join(lines) + "\n", encoding="utf-8")


def main(argv: list[str]) -> int:
    buckets = collect()
    current = totals(buckets)

    if "--print" in argv:
        for bucket in ("PROD", "TEST_INLINE", "TEST_FILE"):
            print(f"{bucket} = {current[bucket]}")
            for rel, count in sorted(buckets[bucket].items()):
                print(f"  {count:3d}  {rel}")
        print(f"合计 = {sum(current.values())}")
        return 0

    baseline = read_baseline()
    if not baseline:
        if "--update" in argv:
            write_baseline(current, buckets)
            print(f"OK 已建立基线：{current}")
            return 0
        print(f"FAIL 缺少基线 {BASELINE}（先跑 --update 建立）", file=sys.stderr)
        return 1

    if "--update" in argv:
        worse = [
            bucket for bucket in current if current[bucket] > baseline.get(bucket, 0)
        ]
        if worse:
            detail = ", ".join(
                f"{b} {baseline.get(b, 0)} → {current[b]}" for b in worse
            )
            print(
                f"FAIL --update 只能收紧：{detail}\n"
                "新增 sleep 不是「更新基线」能批准的事 —— 要么去掉它，要么在审计 TEST-05 的分类表里登记理由。",
                file=sys.stderr,
            )
            return 1
        write_baseline(current, buckets)
        print(f"OK 基线已收紧：{current}")
        return 0

    problems: list[str] = []
    for bucket in ("PROD", "TEST_INLINE", "TEST_FILE"):
        before = baseline.get(bucket, 0)
        after = current[bucket]
        if after > before:
            problems.append(f"{bucket} 从 {before} 增到 {after}（+{after - before}）")
    if problems:
        print(
            "FAIL `sleep` 站点增加：\n  - " + "\n  - ".join(problems), file=sys.stderr
        )
        print(
            "  定位：`python3 scripts/ci/check_sleep_ratchet.py --print` 逐文件逐桶列出。\n"
            "  新增的每一处都要回答一句：它等的是**真实事件**还是**固定窗口**？\n"
            "  后者就是下一个 flake —— 改成条件轮询（带 deadline），或写进审计 TEST-05 的分类表并说明理由。",
            file=sys.stderr,
        )
        return 1

    total = sum(current.values())
    print(
        f"OK `sleep` 站点未增加（PROD {current['PROD']} / TEST_INLINE {current['TEST_INLINE']} / "
        f"TEST_FILE {current['TEST_FILE']}，合计 {total}）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
