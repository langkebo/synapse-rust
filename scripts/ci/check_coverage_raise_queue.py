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
| 9 | `entries` 为空时，必须有一份 `closed` 收尾块（`at` / `why` / `measurements`） | **三条都达标就该把台账关掉**；但"删空"与"关掉"是两件事。没有收尾块的空台账 = 任务被静默丢弃（判据 6 逼着你删最后一条，要么给个交代，要么就一直挂着） |
| 10 | `closed.measurements` 里每个文件的实测值都必须 `>= --max-target` | 收尾块是**证据**，不是安慰话。没达标就写 `closed` 等于给"未完成"盖章 |
| 11 | `closed` 存在时 `entries` 必须为空 | 重新开账必须删掉收尾块，否则 `closed` 会变成一句与当前状态矛盾的话 |
| 12 | `closed.measurements` 的每个路径都必须在基线里；基线仍 `< target` 时必须给出 `closed.absorption_pending`；全部 `>= target` 时该字段必须删除 | **"测到了"和"基线收了"是两件事**：关账靠具名腿的实测，基线抬升靠下一轮 CI 的 `max()` 写回。混为一谈就会在基线仍是 0.0% 时对外宣称"机器已确认"；而吸收完了不删说明，又成了第二句与状态矛盾的话 |

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

# `closed.at` 只接受日期：带时间的戳会让"哪天关的账"多出一种解析口径，而这行字只是给人看的。
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

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


def load_queue(path: pathlib.Path) -> tuple[list[dict], object]:
    """Return `(entries, closed)`; `closed` is the optional closure block."""
    with open(path) as f:
        data = json.load(f)
    if not isinstance(data, dict) or "entries" not in data:
        raise ValueError(f"queue {path} has no `entries` array")
    if not isinstance(data["entries"], list):
        raise ValueError(f"queue {path}: `entries` must be a list")
    return data["entries"], data.get("closed")


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


def audit_state(
    entries: list[dict], closed: object, max_target: float = DEFAULT_MAX_TARGET
) -> list[str]:
    """判据 9–11：台账的「开着还是关着」。

    判据 6 会逼着把达标的条目删掉 —— 删到最后一条时，「删空」与「关账」必须被区分开。
    所以空台账只有两种合法形态：要么还有条目，要么带一份**带数字的**收尾块。没有收尾块的
    空台账按「任务被静默丢弃」处理（fail-closed），这正是判据 6 逼出来的那个时刻：
    你手上最后一条也达标了，总得给个交代。
    """
    errors: list[str] = []

    if closed is None:
        if not entries:
            errors.append(
                "`entries` is empty and there is no `closed` block — an empty ledger without a "
                "closure note means the P2 task was dropped silently"
            )
        return errors

    if not isinstance(closed, dict):
        return [f"`closed` must be an object, got {type(closed).__name__}"]

    if entries:
        errors.append(
            f"`closed` is set while {len(entries)} entr(y|ies) remain — a reopened ledger must drop "
            "the closure note, otherwise `closed` contradicts the current state"
        )

    at = closed.get("at")
    if not isinstance(at, str) or DATE_RE.match(at) is None:
        errors.append(f"`closed.at` must be a YYYY-MM-DD date, got {at!r}")

    why = closed.get("why")
    if not isinstance(why, str) or len(why.strip()) < 15:
        errors.append("`closed.why` must say why the ledger is finished (>= 15 chars)")

    measurements = closed.get("measurements")
    if not isinstance(measurements, dict) or not measurements:
        errors.append(
            "`closed.measurements` must map each file to its measured coverage — a closure without "
            "numbers is an assertion, not evidence"
        )
        return errors

    for path, pct in measurements.items():
        if not isinstance(pct, (int, float)):
            errors.append(
                f"`closed.measurements[{path!r}]` must be a number, got {pct!r}"
            )
        elif pct + EPS < max_target:
            errors.append(
                f"`closed.measurements[{path!r}]` records {pct:g}% < target {max_target:g}% — "
                "a closure is only legal once every row is met"
            )

    return errors


def audit_absorption(
    closed: object,
    baseline: dict[str, float],
    max_target: float = DEFAULT_MAX_TARGET,
) -> list[str]:
    """判据 12：收尾块里的数字与**基线是否已吸收**必须彼此一致。

    `closed.measurements` 记的是"在某条具名腿上测到多少"，而 `coverage_baseline.json` 记的是
    "棘轮认可的地板"。两者之间隔着一轮真实的 CI 覆盖率：只有 `Per-file coverage ratchet` 跑到
    `Commit coverage baseline (auto-ratchet)` 那一步，`max()` 才会把 0.0% 抬到实测值。所以：

    * 基线还没吸收 ⇒ 必须写 `absorption_pending` 说清"等哪一轮"，否则关账会被读成
      "机器已经确认过了"；
    * 基线已经吸收 ⇒ 这段说明必须删掉，否则它马上变成又一句与状态矛盾的话
      （与判据 11 同一条理由：现在为真的句子，条件一变就是假的）。
    """
    if not isinstance(closed, dict):
        return []
    measurements = closed.get("measurements")
    if not isinstance(measurements, dict) or not measurements:
        # 判据 9–11 已经在报这个结构问题，这里不重复。
        return []

    errors: list[str] = []
    pending: list[str] = []
    for path, pct in measurements.items():
        if path not in baseline:
            errors.append(
                f"`closed.measurements[{path!r}]`: no entry in coverage_baseline.json — the closure "
                "must name files the ratchet knows, otherwise nothing can ever absorb the number"
            )
            continue
        current = round(baseline[path], 2)
        if current + EPS < max_target:
            pending.append(f"{path} baseline={current:.2f}%")

    note = closed.get("absorption_pending")
    if pending:
        if not isinstance(note, str) or len(note.strip()) < 15:
            errors.append(
                "`closed.absorption_pending` is required while the committed baseline still sits "
                f"below the target ({'; '.join(pending)}) — a closure records what was *measured*, "
                "not what the ratchet has absorbed; say which run is expected to absorb it"
            )
    elif isinstance(note, str) and note.strip():
        errors.append(
            "`closed.absorption_pending` is stale — every measurement is already >= "
            f"{max_target:g}% in coverage_baseline.json, so the ratchet has absorbed them; delete "
            "the note (a caveat that outlives its cause is another sentence contradicting reality)"
        )

    return errors


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


def describe_closure(closed: object) -> str:
    """一行收尾摘要（台账开着时明确说"开着"，避免"--print 没有 closed 行"被读成没关账）。"""
    if not isinstance(closed, dict):
        return "closure: (ledger is open)"
    measurements = closed.get("measurements")
    numbers = (
        ", ".join(
            f"{path} {pct:g}%"
            for path, pct in sorted(measurements.items())
            if isinstance(pct, (int, float))
        )
        if isinstance(measurements, dict)
        else ""
    )
    head = f"closure: closed on {closed.get('at')} — {closed.get('why')}"
    return f"{head} [{numbers}]" if numbers else head


def mutation_check(
    baseline: dict[str, float],
    max_target: float,
    root: pathlib.Path,
) -> int:
    """八次注入，覆盖三类判据：行级 3 条、台账状态 3 条、收尾块与基线 2 条。

    每次只注入一处缺陷，且必须至少命中一条 —— 命中 0 条说明那条判据是空转的。
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

    # 判据 9–11 是「台账状态」的判据，与上面三条「单行判据」分开注入：它们判的是
    # entries 与 closed 的关系，改一行数据表达不出来。
    state_mutations: list[tuple[str, list[dict], object]] = [
        ("an empty ledger with no closure note", [], None),
        (
            "a closure note left on a ledger that still has rows",
            [dict(sample)],
            {
                "at": "2026-10-28",
                "why": "fixture closure long enough to pass the length check",
                "measurements": {sample["path"]: 82.9},
            },
        ),
        (
            "a closure whose recorded measurement is below the target",
            [],
            {
                "at": "2026-10-28",
                "why": "fixture closure long enough to pass the length check",
                "measurements": {sample["path"]: 12.0},
            },
        ),
    ]
    for label, state_entries, closed in state_mutations:
        errors = audit_state(state_entries, closed, max_target)
        hit = len(errors)
        status = "OK  " if hit else "MISS"
        if hit == 0:
            failures += 1
        print(f"{status} {label} -> {hit} problem(s)")
        for line in errors:
            print(f"       {line}")

    # 判据 12 判的是「收尾块与基线」的关系，同样改一行数据表达不出来：要同时给 closed 与基线。
    absorbed = {
        "at": "2026-10-28",
        "why": "fixture closure long enough to pass the length check",
        "measurements": {sample["path"]: 82.9},
    }
    absorption_mutations: list[tuple[str, dict, dict[str, float]]] = [
        (
            "a closure the baseline has not absorbed, with no pending note",
            dict(absorbed),
            {sample["path"]: 0.0},
        ),
        (
            "a pending note left behind after the baseline caught up",
            {
                **absorbed,
                "absorption_pending": "fixture note long enough to pass the length check",
            },
            {sample["path"]: 82.9},
        ),
    ]
    for label, closed, absorption_baseline in absorption_mutations:
        errors = audit_absorption(closed, absorption_baseline, max_target)
        hit = len(errors)
        status = "OK  " if hit else "MISS"
        if hit == 0:
            failures += 1
        print(f"{status} {label} -> {hit} problem(s)")
        for line in errors:
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
        entries, closed = load_queue(args.queue)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"raise queue cannot read {args.queue}: {error}", file=sys.stderr)
        return 2

    errors, blocked, rows = audit(entries, baseline, args.max_target)
    errors = [
        *errors,
        *audit_state(entries, closed, args.max_target),
        *audit_absorption(closed, baseline, args.max_target),
        *(d for d in (describe(e) for e in entries if isinstance(e, dict)) if d),
    ]

    if args.print:
        print_rows(rows, args.max_target)
        print(describe_closure(closed))
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
