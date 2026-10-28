#!/usr/bin/env python3
"""Guards the coverage "raise queue" ledger (`coverage_raise_queue.json`).

## 这份台账为什么存在

`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1 的 P2 要求："给 0% 文件里**有生产入口**的
建『提升到 30%』的一次性任务，逐文件记账"。同一节也写死了**不能怎么做**：把 30% 当绝对下限重贴到
基线已知文件上，就是 2026-09-19 那次 `188 files red` 的翻版（那次不是回归，是从来没达标过）。

所以这里的 `target` **不是 floor**。真正的强制力来自三条机械约束：

1. **棘轮只会收紧** —— `save_baseline()` 对台账内文件同样取 `max(prev, cur)`，实测值一涨，
   基线就跟着涨，`opened_at` 与当前基线的距离自动缩短；
2. **达成必须删条目** —— 基线 `>= target` 时本脚本判红，要求把条目删掉并在提交信息里写实测值。
   没有这条，台账会变成"永远挂着、没人再看"的第二个真相（`AGENTS.md` 铁律 8 的变体）；
3. **基线不得低于记账起点** —— `opened_at > 基线` 说明有人对这个文件用了 `--allow-lower`
   或手改基线。台账声称"从 X 提升到 30"，基线却掉到 X 以下，两边必须对齐：要么撤销下调，
   要么先从台账移除并说明理由。

## 判据

| # | 判据 | 为什么 |
|---|---|---|
| 1 | `entries` 是列表，每条的 `path` / `opened_at` / `target` / `state` / `production_entry` / `plan` / `evidence` 齐全且类型正确 | 缺字段的台账等于没有台账 |
| 2 | `path` 唯一 | 重复条目会让"达成即删"只删掉一条 |
| 3 | `path` 在 `coverage_baseline.json` 里有记录 | 台账只说"基线已知文件的提升"，新文件归 `--new-file-floor` 管 |
| 4 | `opened_at <= 当前基线` | 见上，防止基线被偷偷下调 |
| 5 | `opened_at < target <= --max-target`（默认 30） | 防止把 target 写成 1 来"达成" |
| 6 | `当前基线 < target` | 已达成 ⇒ 必须删条目（防僵尸） |
| 7 | `production_entry` 开头的 `.rs` 文件真实存在 | "有生产入口"是**可验证的前提**，不是抄来的断言 |
| 8 | `plan` 与 `evidence` 非空且 `plan` 至少 15 个字符 | 防止 `plan: "补测试"` 这种没有下一步的记账 |

## 用法

    python3 scripts/ci/check_coverage_raise_queue.py            # 校验（CI / preflight）
    python3 scripts/ci/check_coverage_raise_queue.py --print    # 打印当前进度表
    python3 scripts/ci/check_coverage_raise_queue.py --mutation-check   # 判据自证

退出码：0 = 台账与基线一致；1 = 有需要人工动作的条目（已达成 / 基线被下调）；
2 = 结构或输入文件不可用（fail-closed）。
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_QUEUE = ROOT / "scripts/ci/coverage_raise_queue.json"
DEFAULT_BASELINE = ROOT / "scripts/ci/coverage_baseline.json"

# 本批口径：队列只登记"提升到 30%"这一档。想登记更高目标要另开一个决定，
# 而不是在这个常量上悄悄改数字。
DEFAULT_MAX_TARGET = 30.0

REQUIRED_TEXT = ("path", "state", "production_entry", "plan", "evidence")
STATES = {
    "measurement-stale",
    "measurement-stale-or-needs-tests",
    "needs-tests",
    "in-progress",
}

# `src/server/mod.rs:1 — 说明` ⇒ 取 `src/server/mod.rs`。
ENTRY_FILE_RE = re.compile(r"^([A-Za-z0-9_./-]+\.rs)(?::\d+)?(?:\s|$|—)")

# 覆盖率百分比存两位小数，比较时用同一精度（否则 29.999999 与 30.0 会来回抖）。
EPS = 0.005


def load_baseline(path: pathlib.Path) -> dict[str, float]:
    """`{normalized_path: line_pct}`，格式与 check_file_coverage.load_baseline 一致。

    这里刻意重写而不是 import：守卫要在**基线损坏时退出 2**，而那边的实现对损坏文件
    返回空字典并只打 warning（它需要能继续跑 `--save-baseline`）。两种语义都要保留。
    """
    with open(path) as f:
        data = json.load(f)
    if not isinstance(data, dict) or "files" not in data:
        raise ValueError(f"baseline {path} has no `files` array")
    return {item["path"]: float(item["line_pct"]) for item in data["files"]}


def load_queue(path: pathlib.Path) -> list[dict]:
    with open(path) as f:
        data = json.load(f)
    if not isinstance(data, dict) or "entries" not in data:
        raise ValueError(f"queue {path} has no `entries` array")
    if not isinstance(data["entries"], list):
        raise ValueError(f"queue {path}: `entries` must be a list")
    return data["entries"]


def audit(
    entries: list[dict],
    baseline: dict[str, float],
    max_target: float = DEFAULT_MAX_TARGET,
    root: pathlib.Path = ROOT,
) -> tuple[list[str], list[str], list[dict]]:
    """Return `(errors, blocked, rows)`.

    `blocked` 是"需要人工动作但输入本身合法"的那一类（已达成 / 基线被下调）——
    它们让脚本退出 1，而不是 2：台账没坏，是任务到点了。
    """
    errors: list[str] = []
    blocked: list[str] = []
    rows: list[dict] = []
    seen: dict[str, int] = {}

    if not entries:
        errors.append(
            "`entries` is empty — an empty ledger means the P2 task was dropped silently"
        )

    for index, entry in enumerate(entries):
        where = f"entries[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{where}: must be an object")
            continue

        missing = [k for k in REQUIRED_TEXT if k not in entry]
        missing += [k for k in ("opened_at", "target") if k not in entry]
        if missing:
            errors.append(f"{where}: missing field(s) {', '.join(sorted(missing))}")
            continue

        path = entry["path"]
        where = f"entries[{index}] ({path})"

        for key in REQUIRED_TEXT:
            if not isinstance(entry[key], str) or not entry[key].strip():
                errors.append(f"{where}: `{key}` must be a non-empty string")

        path = entry["path"]
        if path in seen:
            errors.append(
                f"{where}: duplicate path (also entries[{seen[path]}]) — "
                "'achieved ⇒ delete the entry' would only delete one of them"
            )
        seen[path] = index

        try:
            opened_at = float(entry["opened_at"])
            target = float(entry["target"])
        except (TypeError, ValueError):
            errors.append(f"{where}: `opened_at` / `target` must be numbers")
            continue

        if entry.get("state") not in STATES:
            errors.append(
                f"{where}: `state` must be one of {sorted(STATES)}, got {entry.get('state')!r}"
            )

        if target <= opened_at:
            errors.append(
                f"{where}: target {target:g}% is not above opened_at {opened_at:g}% — "
                "a ledger row that raises nothing is not a task"
            )
        if target > max_target + EPS:
            errors.append(
                f"{where}: target {target:g}% exceeds the batch ceiling {max_target:g}% — "
                "this ledger is the 30% tier; open a separate decision for anything higher"
            )

        plan = entry.get("plan", "")
        if isinstance(plan, str) and len(plan.strip()) < 15:
            errors.append(
                f"{where}: `plan` is too short to be a next step ({plan!r}) — "
                "record what will be tested or which command re-measures it"
            )

        if path not in baseline:
            errors.append(
                f"{where}: no entry in coverage_baseline.json — a NEW file is governed by "
                "`--new-file-floor`, not by this ledger"
            )
            continue

        current = round(baseline[path], 2)

        if current >= target - EPS:
            blocked.append(
                f"{where}: baseline is already {current:.2f}% (target {target:g}%) — "
                "delete this entry and record the measured value in the commit message"
            )
        elif opened_at > current + EPS:
            blocked.append(
                f"{where}: opened_at {opened_at:g}% is above the current baseline {current:.2f}% — "
                "the recorded floor was lowered (--allow-lower or a hand edit). Either undo that "
                "lowering or drop this entry with a reason"
            )

        rows.append(
            {
                "path": path,
                "opened_at": opened_at,
                "current": current,
                "target": target,
                "state": entry.get("state", "?"),
                "gap": max(target - current, 0.0),
            }
        )

    return errors, blocked, rows


def describe(entry: dict, root: pathlib.Path = ROOT) -> str | None:
    """`production_entry` 的首个文件不存在时返回错误文本。

    判据 7 的价值在于：P2 的前提是"这些 0% 文件**有生产入口**"。前提是可以用一条
    `ls` 验证的，那就别让它以断言的形式躺在文档里 —— 文件被删/改名后，台账会自己报红。
    """
    raw = entry.get("production_entry")
    if not isinstance(raw, str):
        return None
    match = ENTRY_FILE_RE.match(raw.strip())
    if match is None:
        return f"{entry.get('path')}: `production_entry` must start with a repo-relative .rs path, got {raw!r}"
    candidate = root / match.group(1)
    if not candidate.exists():
        return (
            f"{entry.get('path')}: production_entry names {match.group(1)}, which does not exist — "
            "the 'has a production entry point' premise no longer holds"
        )
    return None


def print_rows(rows: list[dict], max_target: float) -> None:
    if not rows:
        print("raise queue: (no rows)")
        return
    width = max(len(r["path"]) for r in rows)
    print(f"=== Coverage raise queue (batch ceiling {max_target:g}%) ===")
    print(f"  {'path'.ljust(width)}  {'baseline':>8}  {'target':>6}  {'gap':>6}  state")
    for row in sorted(rows, key=lambda r: r["path"]):
        print(
            f"  {row['path'].ljust(width)}  {row['current']:7.2f}%  {row['target']:5.0f}%  "
            f"{row['gap']:5.1f}%  {row['state']}"
        )
    done = [r for r in rows if r["gap"] <= EPS]
    pending = len(rows) - len(done)
    print(
        f"  -> {len(rows)} row(s): {pending} pending, {len(done)} achieved (achieved must be deleted)"
    )
    print()


def mutation_check(
    baseline: dict[str, float],
    max_target: float,
    root: pathlib.Path,
) -> int:
    """三条判据各注入一次真实缺陷，必须各命中至少一条。

    用 fixture 而不是改磁盘上的台账：已经达成的样本在真实数据里**不存在**（存在就说明
    台账该删了），所以"达成即红"这条只能靠合成基线自证。fixture 复刻真实条目的字段，
    只改被判的那一处 —— 这样报出来的问题不会同时命中别的判据。
    """
    sample = {
        "path": "src/server/router.rs",
        "opened_at": 0.0,
        "target": 30.0,
        "state": "measurement-stale",
        "production_entry": "src/server/mod.rs:1 — test fixture",
        "plan": "fixture plan long enough to pass the length check",
        "evidence": "fixture",
    }
    base = {sample["path"]: 0.0}
    mutations: list[tuple[str, dict, dict]] = [
        (
            "a ledger row whose start value is above the current baseline",
            {**sample, "opened_at": 12.0},
            dict(base),
        ),
        (
            "an achieved row left in the ledger",
            dict(sample),
            {sample["path"]: 30.0},
        ),
        (
            "a production entry that names a file which does not exist",
            {
                **sample,
                "production_entry": "src/server/not_a_real_module.rs:1 — fixture",
            },
            dict(base),
        ),
    ]
    failures = 0
    for label, entry, baseline_override in mutations:
        errors, blocked, _rows = audit([entry], baseline_override, max_target, root)
        description = describe(entry, root)
        if description is not None:
            errors = [*errors, description]
        hit = len(errors) + len(blocked)
        status = "OK  " if hit else "MISS"
        if hit == 0:
            failures += 1
        print(f"{status} {label} -> {hit} problem(s)")
        for line in [*errors, *blocked]:
            print(f"       {line}")
    if failures:
        print(
            f"mutation-check FAILED: {failures} mutation(s) went undetected",
            file=sys.stderr,
        )
        return 1
    print("mutation-check OK: every injected defect is reported")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__.splitlines()[0] if __doc__ else ""
    )
    parser.add_argument("--queue", type=pathlib.Path, default=DEFAULT_QUEUE)
    parser.add_argument("--baseline", type=pathlib.Path, default=DEFAULT_BASELINE)
    parser.add_argument(
        "--max-target",
        type=float,
        default=DEFAULT_MAX_TARGET,
        help="这条台账允许登记的最高目标（默认 30，见模块 docstring）",
    )
    parser.add_argument("--print", action="store_true", help="打印进度表后退出 0")
    parser.add_argument(
        "--mutation-check",
        action="store_true",
        help="对三条判据各注入一次缺陷，确认它们真的会红（不需要真实台账）",
    )
    args = parser.parse_args()

    try:
        baseline = load_baseline(args.baseline)
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(
            f"raise queue cannot read baseline {args.baseline}: {error}",
            file=sys.stderr,
        )
        return 2

    if not baseline:
        print(
            f"raise queue: baseline {args.baseline} is empty (fail-closed)",
            file=sys.stderr,
        )
        return 2

    if args.mutation_check:
        return mutation_check(baseline, args.max_target, ROOT)

    try:
        entries = load_queue(args.queue)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"raise queue cannot read {args.queue}: {error}", file=sys.stderr)
        return 2

    errors, blocked, rows = audit(entries, baseline, args.max_target)
    errors = [
        *errors,
        *(d for d in (describe(e) for e in entries if isinstance(e, dict)) if d),
    ]

    if args.print:
        print_rows(rows, args.max_target)
        print(f"queue: {args.queue} ({len(entries)} entries)")
        print(f"baseline: {args.baseline} ({len(baseline)} files)")
        if errors:
            print("\n".join(errors), file=sys.stderr)
        if blocked:
            print("\n".join(blocked), file=sys.stderr)
        return 0

    if errors:
        print("=== Coverage raise queue is malformed ===")
        for line in errors:
            print(f"  {line}")
        print()
        print(f"queue: {args.queue}")
        return 2

    if blocked:
        print("=== Coverage raise queue needs action ===")
        for line in blocked:
            print(f"  {line}")
        print()
        print(
            "  A row reaching its target must be deleted, not kept 'for the record': "
            "the baseline is the record.\n"
            "  See docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md §3.1.3."
        )
        return 1

    print_rows(rows, args.max_target)
    return 0


if __name__ == "__main__":
    sys.exit(main())
