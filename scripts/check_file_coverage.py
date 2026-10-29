#!/usr/bin/env python3
"""Per-file coverage ratchet for lcov reports.

Reads a coverage report, compares per-file line coverage against the committed
baseline, and exits non-zero when a file falls below its floor.

Policy (revised 2026-10-29; see docs/audit/GATE_INTEGRITY_SWEEP_2026-09-19.md §6):
  - A file the baseline already records: **must not regress** below its recorded
    value. Absolute floors are NOT re-applied to it.
  - A file the baseline already records **and that matches `--core-files`**: the
    floor becomes `min(target, prev)` (`[CORE-KNOWN]`) — still "must not
    regress", plus an absolute target that only ratchets **up**: `target` is
    seeded from the recorded value by tier (0–10% → 30, 10–30% → 40,
    30–50% → 55, 50–70% → 70) and re-raised to `floor(cur/5)*5` whenever
    `cur > target`; it is persisted in the baseline entry (`target` field).
    `min()` is what keeps the bootstrap honest: a core file recorded at 3% is
    **not** red overnight for missing 30% (that is the 2026-09-19 `188 files
    red` shape again); it is pinned at 3% until its own coverage climbs past
    the target, after which the target is the real floor.
  - A file seen for the first time (new): core ≥ 70%, otherwise the new-file
    ramp-up floor (30%) — **unless** it matches a `--non-unit-coverable` prefix.
  - Test-support sources (`test_mocks/`, `synapse-test-utils/`, `*test_utils.rs`,
    `*test_isolation.rs`, `*test_schema_guard.rs`, `scripts/bench_harness.rs`)
    are skipped: they are compiled under `cfg(test)`/`test-utils` and consumed by
    *other* crates' tests, so neither coverage leg can measure them — recording a
    permanent 0% would say nothing about the product.
  - `--non-unit-coverable` lists paths the coverage leg cannot execute at all
    (`src/bin/`, `src/main.rs`): CI runs `cargo llvm-cov --lib`, which never
    links or calls a binary entry point, so those files record 0.0% by
    construction and no new-file floor can ever be met. Exempting them removes
    only the NEW-file floors: a baseline-known file is still checked for
    regression first (the exemption cannot hide a regression), exempt files are
    still written back by `--save-baseline`, and each one is printed in the
    run's exemption list. A missing/empty list, or a prefix matching no file on
    disk, is a hard error (exit 2), exactly like `--core-files`.
  - `--raise-queue` is a *reporting* input, not a floor. It names the files the
    audit's P2 task wants lifted to a target (30%), and every run prints their
    current measurement, the gap, and `[RAISE-DONE]` once the target is met (so
    the ledger row gets deleted). It **cannot** change the exit code: making
    `target` an absolute floor is exactly the 2026-09-19 `188 files red` mistake
    (188 files never met it; none had regressed). The ledger's teeth live in
    `scripts/ci/check_coverage_raise_queue.py`, which turns "achieved but still
    listed" into a failure.

Why the baseline-known rule is "no regression" and not `max(prev, global)`:
the previous floor made a freshly bootstrapped baseline unsatisfiable by
construction. Measured 2026-09-19 immediately after committing the first
baseline: exit 1 with 188 files red, every one of them reporting
`was X%, delta=+0.0%` — i.e. not a regression, just an absolute floor the repo
has never met. A ratchet that is red the moment it starts working teaches people
to ignore it (`AGENTS.md` rule 8's converse).

Usage:
  python3 scripts/check_file_coverage.py \\
      --report coverage/lcov.info \\
      --baseline scripts/ci/coverage_baseline.json \\
      --global-floor 40 --new-file-floor 30 \\
      --core-files scripts/ci/core_file_coverage_prefixes.txt \\
      --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt
"""

import argparse
import functools
import json
import os
import pathlib
import sys
from typing import Collection, Dict, List, Optional, Tuple

ROOT = pathlib.Path(__file__).resolve().parents[1]

# Path fragments that mark a source file as test-only support code. Compared
# against the *normalized* path the report yields (e.g.
# `synapse-storage/test_mocks/room.rs`, `synapse-test-utils/lib.rs`).
# 只有声明、没有函数体的文件（`pub trait X { fn f(&self); }`）在 llvm-cov 里只会留下
# 宏展开的 1–3 行，而这些行**不可达**（每个 impl 都自带方法体，没人走默认体）。
# 对它们套"新文件 ≥50% / core ≥70%"是把结构性的 0/2 当成质量缺陷：
# 实测 2026-10-28 全仓仅 3 个文件命中该形态（`synapse-services/src/auth/{credential_auth,
# token_auth}.rs` 各 LF:2/LH:0、`synapse-storage/src/stream_positions.rs` LF:2/LH:0）。
# 只在满足 `LF <= 3` + `LH == 0` + 基线里没有正覆盖时才生效（新文件与基线已知一视同仁，
# 见 `check_file_coverage` 里的注释）；有 1 行命中（33%）仍是真信号，照常判红。
DECL_ONLY_MAX_LINES = 3

TEST_ONLY_MARKERS: Tuple[str, ...] = (
    "test_mocks/",
    "synapse-test-utils/",
    "/test_utils.rs",
    "/test_isolation.rs",
    "/test_schema_guard.rs",
    "scripts/bench_harness.rs",
)

# `[CORE-KNOWN]` 的分档目标（见模块 docstring）。`target` **必须持久化**在基线条目里，
# 不能每轮从 `prev` 重新推导：那样 `target >= prev` 恒成立，`min(target, prev)` 就永远
# 退化成 `prev`，"到点必须达标"那一半规则永远不生效。
CORE_TARGET_TIERS: Tuple[Tuple[float, float], ...] = (
    (10.0, 30.0),
    (30.0, 40.0),
    (50.0, 55.0),
    (70.0, 70.0),
)


def core_target_floor(line_pct: float) -> float:
    """Seed a core file's target from its currently recorded coverage.

    0–10% → 30, 10–30% → 40, 30–50% → 55, 50–70% → 70, ≥70% → 70.

    The tier is a **seed only**: once written to the baseline, `target` can only
    go up (see `save_baseline`), so the ladder converges on its own without
    anyone hand-editing numbers.
    """
    for upper, target in CORE_TARGET_TIERS:
        if line_pct < upper:
            return target
    return CORE_TARGET_TIERS[-1][1]


def is_test_only(path: str) -> bool:
    """True for test-support sources the coverage ratchet must ignore."""
    normalized = path if path.startswith("/") else "/" + path
    return any(marker in normalized for marker in TEST_ONLY_MARKERS)


def load_baseline(
    path: pathlib.Path,
    lines_found: Optional[Dict[str, int]] = None,
    targets: Optional[Dict[str, float]] = None,
) -> Dict[str, float]:
    """Load prior coverage baseline (file_path -> line_pct).

    `lines_found`, when given, is filled with each entry's recorded `LF`
    (executeable line count) so callers can tell **"分母变大"** from
    **"覆盖到的行变少"** — see `save_baseline` for why the pair matters and why a
    legacy entry (written before 2026-10-28) has **no** `lines_found` key.

    `targets`, when given, is filled with each entry's persisted `[CORE-KNOWN]`
    target. Entries written before 2026-10-29 have no `target` key; those files
    are seeded by tier on the next `--save-baseline` run (see
    `core_target_floor`), which is why "missing" and "0" must stay distinct here
    too — a 0 default would look like "target reached long ago".

    老条目没有该字段 ⇒ **不回填、不猜**。`lines_found[p] = 0` 之类的默认值会让
    "未知"读成"零行"，而那正好是分类器最需要区分的两种处境之一。

    A truncated / zero-byte / non-JSON baseline is treated as **no baseline**
    rather than crashing: `json.load` used to raise `JSONDecodeError` straight out
    of `main`, so the run died with a traceback and exit 1 (measured 2026-09-19)
    and `require_baseline`'s "baseline is empty … re-bootstrap it" message was
    unreachable for exactly the file shape that message describes. Treating it as
    empty keeps the ratchet fail-closed (exit 2 with an actionable message) while
    still letting `--save-baseline` overwrite the corrupt file.
    """
    if not path.exists():
        return {}
    try:
        with open(path) as f:
            data = json.load(f)
    except (json.JSONDecodeError, OSError) as error:
        print(
            f"warning: coverage baseline {path} is unreadable ({error}); treating it as empty",
            file=sys.stderr,
        )
        return {}
    if isinstance(data, dict) and "files" in data:
        if lines_found is not None or targets is not None:
            for item in data["files"]:
                if lines_found is not None:
                    lf = item.get("lines_found")
                    if isinstance(lf, int) and lf > 0:
                        lines_found[item["path"]] = lf
                if targets is not None:
                    target = item.get("target")
                    if isinstance(target, (int, float)) and target > 0:
                        targets[item["path"]] = float(target)
        return {item["path"]: item["line_pct"] for item in data["files"]}
    if isinstance(data, dict):
        return data
    return {}


def require_baseline(path: pathlib.Path, baseline: Dict[str, float]) -> Optional[str]:
    """Refuse to run the ratchet without a baseline.

    With an empty baseline every file takes the `is_new` branch
    (`floor = new_file_threshold`, 30%) and the `TOUCHED` branch
    (`floor = max(prev, global_threshold)`) is **unreachable** — so "a touched
    file must not regress" was never enforced while the gate still reported a
    normal verdict. The four sibling ratchets in this repo
    (`.fmt-baseline`, `.missing-docs-baseline`, `trait_count_baseline`,
    `sqlx_dynamic_ratio_baseline`) all fail closed when their baseline is
    missing; this one silently degraded instead, and its baseline is the only one
    that was never committed to the repo, so the branch had never run.

    Returns an error message when the ratchet cannot be enforced, else None.
    """
    if not path.exists():
        return (
            f"coverage baseline not found: {path}\n"
            "  The per-file ratchet cannot enforce 'touched files must not regress'\n"
            "  without it, and would silently treat every file as new.\n"
            "  Bootstrap it once and commit the result:\n"
            "    python3 scripts/check_file_coverage.py --report coverage/lcov.info \\\n"
            "      --baseline " + str(path) + " --save-baseline " + str(path) + " \\\n"
            "      --threshold 0 --global-floor 0 --new-file-floor 0 --core-threshold 0"
        )
    if not baseline:
        return (
            f"coverage baseline is empty: {path}\n"
            "  An empty baseline makes every file 'new', which disables the\n"
            "  regression check. Re-bootstrap it (see the command above)."
        )
    return None


def _baseline_entry(
    path_key: str,
    line_pct: float,
    lines_found: Optional[int],
    target: Optional[float] = None,
) -> dict:
    """One `files[]` row. `lines_found`/`target` 只在**已知且与 line_pct 同源**时写出。"""
    entry = {"path": path_key, "line_pct": round(line_pct, 2)}
    if lines_found is not None:
        entry["lines_found"] = int(lines_found)
    if target is not None:
        entry["target"] = round(float(target), 2)
    return entry


def save_baseline(
    path: pathlib.Path,
    files: Dict[str, float],
    previous: Optional[Dict[str, float]] = None,
    allow_lower: Collection[str] = (),
    lines_found: Optional[Dict[str, int]] = None,
    previous_lines: Optional[Dict[str, int]] = None,
    targets: Optional[Dict[str, float]] = None,
    core_prefixes: Optional[List[str]] = None,
) -> int:
    """Save the coverage snapshot, never LOWERING a recorded floor — unless told to.

    A ratchet must be monotone. The previous version overwrote every entry with
    the current number, so a file whose coverage dropped had its floor lowered to
    the new, worse value — the next run would then accept the regression. With
    `previous`, each entry keeps `max(previous, current)`, so the baseline can
    only tighten. Deliberately lowering a floor is an explicit edit of this file.

    ## `previous` 里**没出现在本次报告**的条目必须保留（2026-10-25 修复）

    此前实现是 `{p: max(prev, cur) for p, cur in files.items()}` —— 只遍历**本次 lcov**
    的键，于是 `previous` 里多出来的条目被整批丢掉。

    而调用方 `main()` 的 `save_path = args.save_baseline or args.baseline` 意味着
    **不传 `--save-baseline` 也会写回基线**（CI 的 "Per-file coverage ratchet (B.3)"
    正是靠这个默认行为在 main 上自动收紧）。两者叠加后的真实故障：拿一份**不完整**的
    lcov（只跑了几个测试、或做门槛验证用的最小报告）跑一次，608 条基线会被截断成 1 条 ——
    棘轮当场报废，之后所有历史文件都变"新文件"，只受 ramp-up 门槛约束。

    实测（2026-10-25）：用 `SF:synapse-common/src/ramp_up_probe.rs` 一条假记录的 lcov
    跑门槛验证，`coverage_baseline.json` 从 608 条变成 1 条。见
    `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1.1。

    代价：文件被删除 / 重命名后旧条目会留在基线里（棘轮只增不减，无害；重命名后的新路径
    按"新文件"受 ramp-up 约束，那本来就是 policy 想要的压力）。

    ## `allow_lower`：合法重构的唯一出口（2026-10-28）

    `max(prev, cur)` 是**上界只降不升**的语义，于是"合法删代码"没有合规路径：

    * 把一个 600 行文件里**已覆盖**的逻辑抽到新文件，原文件剩下的多是不易覆盖的分支
      ⇒ 该文件覆盖率下降，但新文件受 ramp-up 门槛约束，整体覆盖没变差；
    * 原文件的 floor 仍是历史最高值 ⇒ **永久红**；
    * `--save-baseline` 也救不了（`max` 会把旧值写回去），唯一出路是手改 54 KB 的 JSON，
      而那次手改在 review 里看起来和"偷偷放宽门禁"没有区别。

    `allow_lower` 把这条出口**显式化**：只有命令行逐条点名（`--allow-lower <path>`）的
    文件才允许把 floor 下调到本次实测值，其余一律继续 `max`。默认空集 ⇒ CI 路径零变化。
    向下调多少、为什么，都在提交信息与 review diff 里可见（`coverage_baseline.json`
    的那一行就是审计记录）。

    ## `lines_found`：把 `line_pct` 的**分母**一起记下来（2026-10-28）

    只存 `line_pct` 时，"分母变大"与"覆盖到的行变少"在棘轮眼里**完全同形**：

    * 文件从 769 行涨到 1200 行、命中行从 283 涨到 400 ⇒ 59.47% → 33.3%，
      这是**新代码没被覆盖**，不是退化；
    * 文件行数不变、命中行从 283 掉到 200 ⇒ 59.47% → 36.5%，
      这是**覆盖它的测试不再跑**，是退化。

    实测（2026-10-28，第一份完整报告 674 个文件）两条最大的下降正好各占一边：
    `synapse-federation/src/client.rs`（LF 769、五次改动）与
    `synapse-storage/src/event/txn_dedup.rs`（LF 45、零改动）。
    基线多存一个 `LF` 就能把这两类判开，代价是 1 个整数/条目。

    **必须成对**：`line_pct` 与 `lines_found` 要来自**同一次测量**，否则
    "59.47% × 769 行"这种拼出来的组合会凭空造出 457 行命中，而真实值可能是别的。
    所以：

    * 本次更严（`cur >= prev`）⇒ floor 与它的分母一起换成这一对；
    * 本次更低（floor 保持不变）⇒ **连同旧分母一起留着**；旧条目没有分母时
      ⇒ 就让它没有（未知），绝不拿本次的 LF 去配旧的 pct。

    代价（明确接受）：`2026-10-28` 之前写下的条目一旦落在"低于 floor"那一支，它的
    分母永远补不上 —— 除非哪天被 `--allow-lower` 点名或覆盖率回到 floor 之上。
    这类条目在 `classify_coverage_drift.py` 里会标成 `LF-UNKNOWN`，不会被当成回归。

    ## `target`：`[CORE-KNOWN]` 的绝对目标，只升不降（2026-10-29）

    匹配 `core_prefixes` 的条目多存一个 `target`，供 `check_file_coverage` 计算
    `floor = min(target, prev)`（"不许跌" + "到点必须达标"）。规则：

    * 首次（条目里没有 `target`）⇒ 用 `core_target_floor(prev)` 按档位播种；
    * 之后每轮 ⇒ `cur > target` 时抬到 `floor(cur/5)*5`，否则保持；
    * 本次报告里没有它 ⇒ 保留旧 `target`（与 `line_pct` 同一条不删规则）；
    * 不匹配 core 前缀的条目**不写**该字段（清单是唯一真源，移出清单即移出规则）。

    `floor()` 而不是 `ceil()`：目标往"整 5%"上靠，不为了多 1 个百分点把下一条阶梯
    提前拉满；`max()` 保证它单调，没人需要手改数字。

    返回被下调的条目数。
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    previous = previous or {}
    lines_found = lines_found or {}
    previous_lines = previous_lines or {}
    targets = targets or {}
    core_prefixes = core_prefixes or []
    lower_set = set(allow_lower)
    merged: Dict[str, float] = {}
    merged_lines: Dict[str, int] = {}
    lowered = 0
    for path_key in set(previous) | set(files):
        prev_value = float(previous.get(path_key, 0.0))
        cur_value = files.get(path_key)
        prev_lf = previous_lines.get(path_key)
        if cur_value is None:
            # 本次报告里没有它 ⇒ 保留旧条目（不删），连它的分母一起
            merged[path_key] = prev_value
            if prev_lf is not None:
                merged_lines[path_key] = prev_lf
            continue
        cur_value = float(cur_value)
        cur_lf = lines_found.get(path_key)
        if path_key in lower_set or cur_value >= prev_value:
            # floor 换成本次值（显式下调，或棘轮正常收紧）⇒ 分母跟着换，成对
            if path_key in lower_set and cur_value < prev_value:
                lowered += 1
            merged[path_key] = cur_value
            if cur_lf is not None:
                merged_lines[path_key] = cur_lf
            continue
        # floor 保持旧值 ⇒ 只保留**旧的**分母；旧条目没有就保持未知
        merged[path_key] = prev_value
        if prev_lf is not None:
            merged_lines[path_key] = prev_lf
    merged_targets: Dict[str, float] = {}
    for path_key in merged:
        if not core_prefixes or not _matches_prefix(path_key, core_prefixes):
            continue
        recorded = targets.get(path_key)
        target = (
            float(recorded)
            if recorded is not None
            else core_target_floor(float(previous.get(path_key, 0.0)))
        )
        cur_value = files.get(path_key)
        if cur_value is not None and float(cur_value) > target:
            # Only raise, and only to the 5% grid: the ladder converges on its own.
            target = max(target, (float(cur_value) // 5.0) * 5.0)
        merged_targets[path_key] = target
    payload = {
        "files": [
            _baseline_entry(p, merged[p], merged_lines.get(p), merged_targets.get(p))
            for p in sorted(merged)
        ]
    }
    with open(path, "w") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")
    return lowered


def load_prefix_list(path: Optional[pathlib.Path]) -> List[str]:
    """Load list of path prefixes (one per line, supports dir/ prefix matching).

    Shared by `--core-files` and `--non-unit-coverable`; comments (`#`) and blank
    lines are ignored.
    """
    if path is None or not path.exists():
        return []
    with open(path) as f:
        return [line.strip() for line in f if line.strip() and not line.startswith("#")]


@functools.lru_cache(maxsize=1)
def _normalized_source_paths() -> List[str]:
    """Normalized paths of every .rs file in the repo (excluding build/vendor).

    Skipped directories are pruned *before* descending. The previous
    `ROOT.rglob("*.rs")` walked `target/` first (hundreds of thousands of files)
    and only filtered afterwards, so a concurrent cargo build rotating a
    `target/**/*.rmeta*` file mid-walk raised a bare `FileNotFoundError` — the
    guard went red for a filesystem race instead of a coverage verdict.
    """
    skip = {"target", "vendor", ".git", ".claude", "node_modules"}
    out: List[str] = []
    for dirpath, dirnames, filenames in os.walk(str(ROOT)):
        dirnames[:] = [d for d in dirnames if d not in skip]
        for name in filenames:
            if name.endswith(".rs"):
                out.append(_normalize_path(f"{dirpath}/{name}"))
    return out


def stale_prefixes(prefixes: List[str]) -> List[str]:
    """Prefixes that match no file on disk.

    Used for both `--core-files` and `--non-unit-coverable`. Each list was
    historically a gitignored, never-generated artifact, so a threshold could
    silently apply to zero files in CI. A prefix that matches nothing means the
    guard is dead — fail loudly instead of passing vacuously.
    """
    if not prefixes:
        return []
    normalized = _normalized_source_paths()
    return [p for p in prefixes if not any(n.startswith(p) for n in normalized)]


def _matches_prefix(path: str, prefixes: List[str]) -> bool:
    """Check if a path matches any prefix in `prefixes` (directory or file prefix)."""
    for prefix in prefixes:
        if path.startswith(prefix):
            return True
    return False


def parse_lcov(
    report_path: pathlib.Path,
    lines_found: Optional[Dict[str, int]] = None,
) -> Dict[str, float]:
    """Parse an lcov.info report and return {rel_path: line_pct}.

    lcov records use ``SF:<path>`` / ``LF:<lines found>`` / ``LH:<lines hit>``,
    terminated by ``end_of_record``. The SF path is absolute (cargo llvm-cov)
    or repo-relative; ``_normalize_path`` reduces both to the same
    crate-qualified key (``synapse-services/error.rs``), so a file in one crate
    can no longer overwrite the same-named file in another.
    """
    result: Dict[str, float] = {}
    current_path: Optional[str] = None
    current_lf = 0
    current_lh = 0

    def flush() -> None:
        nonlocal current_path, current_lf, current_lh
        if current_path is None:
            return
        rel = _normalize_path(current_path)
        if rel and _is_src_rs(rel):
            result[rel] = (current_lh / current_lf * 100.0) if current_lf > 0 else 0.0
            if lines_found is not None:
                lines_found[rel] = current_lf
        current_path = None
        current_lf = 0
        current_lh = 0

    with open(report_path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("SF:"):
                current_path = line[3:]
            elif line.startswith("LF:"):
                try:
                    current_lf = int(line[3:])
                except ValueError:
                    current_lf = 0
            elif line.startswith("LH:"):
                try:
                    current_lh = int(line[3:])
                except ValueError:
                    current_lh = 0
            elif line == "end_of_record" or line == "end_of_record:":
                flush()
    flush()
    return result


def _normalize_path(p: str) -> str:
    """Return a repo-unique key for a Rust source path.

    The previous implementation returned everything after the FIRST ``src/``.
    That collapsed all nine crates into one namespace: measured on the real tree,
    `synapse-services/src/error.rs`, `synapse-cache/src/error.rs` and
    `synapse-common/src/error.rs` all became `error.rs`, and `lib.rs` was claimed
    by all nine `src/lib.rs` files — 23 colliding keys in total.

    Callers store these keys in a dict (`result[rel] = ...`), so the last writer
    won and the other crates' files were **never measured**, while the ratchet
    compared whichever number survived. It also made the crate-agnostic prefixes
    in `scripts/ci/core_file_coverage_prefixes.txt` match unintended crates.

    Keep the crate directory as a prefix so identity is preserved::

        synapse-services/src/auth/mod.rs -> synapse-services/auth/mod.rs
        src/lib.rs                       -> src/lib.rs          (root crate)
        tests/integration/mod.rs         -> tests/integration/mod.rs
    """
    p = p.replace("\\", "/")
    root = str(ROOT).replace("\\", "/").rstrip("/")
    if p.startswith(root + "/"):
        rel = p[len(root) + 1 :]
    elif p.startswith("/"):
        # Absolute but outside the repo (should not happen for this repo's own
        # coverage reports). Return the full path: it is unique by construction,
        # so it can never silently overwrite another file's entry.
        return p.lstrip("/")
    else:
        # Already repo-relative (`lcov` paths differ from `Path.rglob` output)
        # which is absolute). Normalise both to the same key.
        rel = p[2:] if p.startswith("./") else p

    if rel.startswith("src/") or rel.startswith("tests/"):
        return rel

    marker = "/src/"
    idx = rel.find(marker)
    if idx > 0:
        crate = rel[:idx]
        # A single leading segment means a workspace crate (`synapse-web`). A
        # nested prefix means some other layout; fall through untouched.
        if "/" not in crate:
            return f"{crate}/{rel[idx + len(marker) :]}"
    return rel


def _is_src_rs(rel: str) -> bool:
    """Only enforce thresholds on Rust source files under src/."""
    return rel.endswith(".rs") and not rel.startswith("tests/")


def _compute_line_pct(coverage: list) -> float:
    """Given lcov LF/LH counters, return line coverage percentage."""
    if not coverage:
        return 0.0
    covered = sum(1 for entry in coverage if _count(entry) > 0)
    total = len(coverage)
    return (covered / total) * 100.0 if total > 0 else 0.0


def _count(entry) -> int:
    """Extract the hit count from an lcov LH value."""
    if isinstance(entry, dict):
        return entry.get("count", 0)
    if isinstance(entry, (int, float)):
        return int(entry)
    return 0


def check_file_coverage(
    current: Dict[str, float],
    baseline: Dict[str, float],
    global_threshold: float,
    new_file_threshold: float,
    core_prefixes: List[str],
    core_threshold: float,
    non_unit_prefixes: List[str],
    report_path: pathlib.Path,
    allow_lower: Collection[str] = (),
    lines_found: Optional[Dict[str, int]] = None,
    core_targets: Optional[Dict[str, float]] = None,
) -> int:
    """Enforce per-file coverage thresholds.  Returns exit code.

    Priority (highest wins): baseline-known (no regression) > core > new >
    non-unit-coverable exemption > declaration-only exemption.

    The exemption is checked **after** the baseline-known branch on purpose: a
    file the coverage leg cannot execute is still forbidden from regressing below
    its recorded value, so the list can never hide a regression.

    `allow_lower` is the explicit escape hatch for a deliberately lowered floor
    (see `save_baseline`). A path listed there is still reported — as
    `[LOWERED]`, not as a silent pass — so the drop stays visible in the log.

    A baseline-known file under `--core-files` gets `[CORE-KNOWN]`:
    `floor = min(target, prev)` with `target` read from `core_targets` (persisted
    by `save_baseline`). See the module docstring for why `min()` and not
    `target` alone — a core file recorded at 3% must not go red overnight.
    """
    failures: List[str] = []
    warnings: List[str] = []
    core_failures: List[str] = []
    exemptions: List[str] = []
    lowered: List[str] = []
    core_known_total = 0
    core_known_met = 0
    lower_set = set(allow_lower)
    all_paths = sorted(set(current.keys()) | set(baseline.keys()))

    for path in all_paths:
        cur = current.get(path)
        prev = baseline.get(path)

        if cur is None:
            continue

        # Test-support sources are structurally unmeasurable here (see
        # TEST_ONLY_MARKERS); the ratchet must not record a meaningless 0%.
        if is_test_only(path):
            continue

        # 与基线**同一精度**比较：基线是用 `round(v, 2)` 存的（见 save_baseline），
        # 而这里的 cur 是未取整的原始比值。不取整就会出现"同一份覆盖率的
        # 往返不对称"：66.67（存储）vs 66.666…（实时）⇒ `cur < prev` 成立，
        # 每个百分比不能用两位小数精确表示的文件（2/3、1/3、1/7…）都会永远
        # 报 `[TOUCHED] … < … (delta=-0.0%)`，棘轮一旦有基线就恒红
        # （实测 2026-09-19）。取整到存储精度后往返无损，`delta` 也不再显示 -0.0。
        cur = round(cur, 2)

        is_new = prev is None

        # 声明文件（trait/struct/enum 只有签名，没有函数体）在 llvm-cov 里只剩宏展开的
        # 1–3 行，且不可达 ⇒ 结构性 0%，与"没写测试"无关。详见 `DECL_ONLY_MAX_LINES`。
        #
        # 对**基线已知**的文件同样生效（2026-10-28）：否则同一个声明文件只因为"进过基线"
        # 就从 `[DECL-ONLY]` 掉进 `[TOUCHED]` 分支 —— 结论一样（0.0 不低于 0.0），但豁免
        # 不再被打印，审计侧根本数不出到底有几个结构性 0% 的文件。
        # 回归保护不放松：`prev > 0` 说明这个文件曾经有行被命中过，那它就不是声明文件，
        # 0% 是真跌，照旧走下面的回归比较。
        coverable = (lines_found or {}).get(path, 0)
        if (
            coverable <= DECL_ONLY_MAX_LINES
            and cur == 0.0
            and (prev is None or prev == 0.0)
        ):
            exemptions.append(
                f"[DECL-ONLY] {path}: {coverable} coverable line(s), none reachable "
                "(declaration-only file: no function bodies to execute)"
            )
            continue

        if not is_new:
            # A file the baseline already knows: only regression matters. See the
            # module docstring for why `max(prev, global_threshold)` was wrong.
            #
            # core 例外：[CORE-KNOWN] 在"不许跌"之外再挂一个只升不降的绝对目标，
            # floor 取 `min(target, prev)` —— 见模块 docstring 与 `core_target_floor`。
            if _matches_prefix(path, core_prefixes):
                target = (core_targets or {}).get(path, core_target_floor(prev))
                floor = min(target, prev)
                tag = "CORE-KNOWN"
                core_known_total += 1
                if cur + 1e-9 >= target:
                    core_known_met += 1
            else:
                floor = prev
                tag = "TOUCHED"
        elif _matches_prefix(path, non_unit_prefixes):
            # Paths the coverage leg cannot execute (`cargo llvm-cov --lib` never
            # runs a binary entry point, see the module docstring). No floor can
            # ever be met, so none is applied — but the baseline-known branch
            # above already ran, and these files are still saved to the baseline
            # and listed here so the exemption is never silent.
            exemptions.append(
                f"[EXEMPT] {path}: {cur:.1f}% (new file under a --non-unit-coverable "
                f"prefix; the coverage leg cannot execute it)"
            )
            continue
        elif _matches_prefix(path, core_prefixes):
            floor = core_threshold
            tag = "CORE"
        else:
            floor = new_file_threshold
            tag = "NEW"

        if cur < floor:
            delta = cur - (prev or 0.0)
            # `prev` is None for a file with no baseline (a NEW file that also
            # matches a core prefix lands in the CORE branch above). Formatting
            # it directly raised TypeError and crashed the gate instead of
            # reporting the shortfall.
            prev_txt = f"{prev:.1f}%" if prev is not None else "no baseline"
            target_txt = f", target {target:.0f}%" if tag == "CORE-KNOWN" else ""
            msg = (
                f"[{tag}] {path}: {cur:.1f}% < {floor:.0f}%{target_txt} "
                f"(was {prev_txt}, delta={delta:+.1f}%)"
            )
            # 显式授权的下调不判红，但**必须打印**：静默放行与"门禁没在工作"
            # 无法区分（铁律 8）。
            if path in lower_set:
                lowered.append(f"{msg} — floor lowered on purpose (--allow-lower)")
                continue
            if tag in ("CORE", "CORE-KNOWN"):
                core_failures.append(msg)
            else:
                failures.append(msg)
        elif is_new and cur < global_threshold:
            warnings.append(
                f"[{tag}] {path}: {cur:.1f}% (below global {global_threshold:.0f}% "
                f"but above new-file ramp-up {new_file_threshold:.0f}%)"
            )

    if core_known_total:
        print(
            f"Core-known guard: {core_known_total} baseline-known core file(s), "
            f"{core_known_met} already at/above target "
            "(floor = min(target, recorded); target only ratchets up)."
        )

    if lowered:
        print(
            f"=== Floors lowered on purpose (--allow-lower, {len(lowered)} file(s)) ==="
        )
        for entry in lowered:
            print(f"  {entry}")
        print()

    if exemptions:
        print("=== Non-unit-coverable exemptions (new files; no floor applied) ===")
        for e in exemptions:
            print(f"  {e}")
        print()

    if warnings:
        print("=== Coverage warnings (ramp-up grace) ===")
        for w in warnings:
            print(f"  {w}")
        print()

    if core_failures:
        print(f"=== Core-path coverage failures ({len(core_failures)} files) ===")
        for f in core_failures:
            print(f"  {f}")
        print()

    if failures:
        print(f"=== Coverage failures ({len(failures)} files) ===")
        for f in failures:
            print(f"  {f}")
        print()

    if failures or core_failures:
        print(
            f"Thresholds: baseline-known files must not regress "
            f"(core ones: floor = min(target, recorded)); "
            f"new files: core ≥{core_threshold:.0f}%, "
            f"otherwise ≥{new_file_threshold:.0f}% (ramp-up); "
            f"global ≥{global_threshold:.0f}% is a warning only."
        )
        return 1

    print(
        f"All {len(current)} source files meet coverage thresholds "
        f"(baseline-known: no regression, core ones also ≥ min(target, recorded); "
        f"new: core≥{core_threshold:.0f}%, "
        f"otherwise≥{new_file_threshold:.0f}%; "
        f"non-unit-coverable exempted: {len(exemptions)})."
    )
    return 0


def load_raise_queue(path: pathlib.Path) -> Tuple[List[dict], Optional[dict]]:
    """Load the coverage raise ledger → `(entries, closed)`.

    Fail-closed like `--core-files` / `--non-unit-coverable`: a missing file, a
    missing `entries` array, or an empty list **without a closure block** all mean
    the P2 task silently stopped being reported — the state nobody notices.

    ## `entries: []` + `closed` 是合法终态（2026-10-28 修正）

    台账的口径是「达成即删」：最后一条达标时条目会被删光，数字搬进 `closed`
    （`at` / `why` / `measurements`）。而本函数的旧实现在 `entries` 为空时**无条件**
    判红，于是"按规矩关账"反而把整条主车道弄红：实测 run 40649970973 的
    `Per-file coverage ratchet` 就是红在这句话上（
    `raise queue … is empty — the 30% lift task was dropped silently`），而报告里
    674 个文件根本没被比较过。**判据把"完成"读成了"丢失"**。

    所以现在区分两件事：
      * 空 + 有 `closed` ⇒ 关账完成，指标只打印 `[RAISE-CLOSED]`，不参与退出码；
      * 空 + 无 `closed` ⇒ 仍是 exit 2（"任务被静默丢弃"），这条不变量不变。
    结构校验（`closed` 的字段与"达标才允许关账"）由 `scripts/ci/check_coverage_raise_queue.py`
    负责，本函数只认"有没有"这一层。
    """
    if not path.exists():
        raise ValueError(f"raise queue not found: {path}")
    with open(path) as f:
        data = json.load(f)
    if not isinstance(data, dict) or not isinstance(data.get("entries"), list):
        raise ValueError(f"raise queue {path} has no `entries` array")
    closed = data.get("closed")
    closed = closed if isinstance(closed, dict) and closed else None
    if not data["entries"]:
        if closed is None:
            raise ValueError(
                f"raise queue {path} is empty and carries no `closed` block — "
                "the 30% lift task was dropped silently"
            )
        return [], closed
    return data["entries"], closed


def report_raise_queue(
    current: Dict[str, float],
    entries: List[dict],
    closed: Optional[dict] = None,
) -> None:
    """Print progress for the `coverage_raise_queue.json` entries.

    **Deliberately cannot affect the exit code.** `target` is an aspiration, not
    a floor: re-applying an absolute floor to baseline-known files made the
    first committed baseline red on 188 files that had not regressed at all
    (measured 2026-09-19) — the very outcome `coverage_policy.md` forbids. The
    ledger's enforcement is separate: `check_coverage_raise_queue.py` fails when
    a row has reached its target and is still listed, and when a recorded start
    value is above the current baseline (i.e. the floor was quietly lowered).

    Three extra states matter here:

    * `[RAISE-CLOSED]`: the ledger is finished (`entries: []` + a `closed` block
      recording the measured values). Printed so a closed task stays visible
      instead of looking like "someone deleted the file";
    * `[RAISE-NODATA]`: the file is missing from *this* report. A file that is
      silently absent is not "0%", it is "not measured" — the same failure shape
      as the `594 个条目保留自上次` shrinkage, so it gets its own tag;
    * `[RAISE-DONE]`: reached the target ⇒ delete the ledger row in the same
      commit that lets the ratchet tighten the baseline.
    """
    if not entries:
        if closed:
            measurements = closed.get("measurements")
            numbers = (
                ", ".join(
                    f"{p} {v:g}%"
                    for p, v in sorted(measurements.items())
                    if isinstance(v, (int, float))
                )
                if isinstance(measurements, dict)
                else ""
            )
            print("=== Coverage raise queue: 已关账（entries 为空 + closed 收尾块）===")
            print(f"  [RAISE-CLOSED] {closed.get('at')} — {closed.get('why')}")
            if numbers:
                print(f"  封存实测值：{numbers}")
            tail = closed.get("absorption_pending")
            if isinstance(tail, str) and tail.strip():
                print(f"  待吸收：{tail}")
        return
    print("=== Coverage raise queue (progress only; `target` is NOT a floor) ===")
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        path = entry.get("path")
        target = entry.get("target")
        if not isinstance(path, str) or not isinstance(target, (int, float)):
            continue
        target = float(target)
        cur = current.get(path)
        if cur is None:
            print(
                f"  [RAISE-NODATA] {path}: 本次报告里没有该文件（目标 {target:.0f}%）"
                " — 先查 classify_coverage_drift.py 的 GONE-* 成因，别当成 0%"
            )
            continue
        cur = round(cur, 2)
        if cur >= target - 0.005:
            print(
                f"  [RAISE-DONE] {path}: {cur:.1f}% ≥ {target:.0f}% — "
                "删掉 coverage_raise_queue.json 里的这一条（基线会本轮收紧）"
            )
        else:
            print(
                f"  [RAISE] {path}: {cur:.1f}% → {target:.0f}%"
                f"（还差 {target - cur:.1f} 个百分点）"
            )
    print()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Enforce per-file coverage thresholds from an lcov report."
    )
    parser.add_argument(
        "--report",
        required=True,
        type=pathlib.Path,
        help="Path to coverage report (lcov.info).",
    )
    parser.add_argument(
        "--baseline",
        required=True,
        type=pathlib.Path,
        help="Path to prior coverage baseline JSON (created if missing).",
    )
    parser.add_argument(
        "--global-floor",
        type=float,
        default=70.0,
        help="Global coverage floor, used only for the below-global warning on new files (default: 70).",
    )
    parser.add_argument(
        "--new-file-floor",
        type=float,
        default=60.0,
        help="Ramp-up coverage floor for files without a baseline (default: 60).",
    )
    parser.add_argument(
        "--core-files",
        type=pathlib.Path,
        default=None,
        help="File listing core security paths (one prefix per line, matched by prefix).",
    )
    parser.add_argument(
        "--core-threshold",
        type=float,
        default=70.0,
        help=(
            "Coverage floor for NEW files under a core prefix (default: 70). "
            "Does NOT re-apply a 70%% floor to core files already in the "
            "baseline — those are governed by [CORE-KNOWN] "
            "(cur >= min(target, prev)), where `target` is seeded per band, "
            "only ratchets up, and is persisted in the baseline."
        ),
    )
    parser.add_argument(
        "--non-unit-coverable",
        type=pathlib.Path,
        default=None,
        help=(
            "File listing path prefixes the unit coverage leg cannot execute "
            "(one prefix per line, matched by prefix). New files under these "
            "prefixes are exempt from the new-file floors; baseline-known "
            "regression checks still apply."
        ),
    )
    parser.add_argument(
        "--raise-queue",
        type=pathlib.Path,
        default=None,
        help=(
            "Path to the coverage raise ledger (JSON with an `entries` array). "
            "Every run prints each entry's measured value, its gap to `target`, "
            "and [RAISE-DONE] once met. REPORTING ONLY — it never changes the "
            "exit code; see the module docstring for why `target` is not a floor."
        ),
    )
    parser.add_argument(
        "--save-baseline",
        type=pathlib.Path,
        default=None,
        help="Path to write the updated baseline snapshot (default: overwrite --baseline).",
    )
    parser.add_argument(
        "--allow-lower",
        action="append",
        default=[],
        metavar="PATH",
        help=(
            "Explicitly allow this file's floor to be lowered to the current "
            "measurement (repeatable). Use it for a legitimate refactor that "
            "moves covered logic out of a file; every other file keeps the "
            "monotone `max(previous, current)` rule. Each PATH must already "
            "exist in the baseline — a typo would otherwise be a silent no-op."
        ),
    )
    args = parser.parse_args()

    if not args.report.exists():
        print(f"Coverage report not found: {args.report}", file=sys.stderr)
        return 1

    lines_found: Dict[str, int] = {}
    current = parse_lcov(args.report, lines_found)
    baseline_lines: Dict[str, int] = {}
    baseline_targets: Dict[str, float] = {}
    baseline = load_baseline(args.baseline, baseline_lines, baseline_targets)

    # A ratchet without its baseline cannot enforce anything (see
    # `require_baseline`). Checked before the expensive per-file loop so the
    # failure is unambiguous.
    #
    # ⚠️ Bootstrap must be exempt, otherwise the gate deadlocks: `require_baseline`
    # used to run unconditionally, so the very command it tells the operator to run
    # (`--baseline X --save-baseline X` on a missing X) exited 2 **before**
    # `save_baseline` could create X — and CI's bootstrap step
    # (`ci.yml`, "Bootstrap coverage baseline") is exactly that command, so
    # artifacts/coverage_baseline.json was never produced and the per-file ratchet
    # never evaluated a single file (measured 2026-09-19).
    #
    # Bootstrap means: the same path is read and written, and it is missing or
    # empty, so creating it *is* the operation. Reading one path while writing
    # another still requires the read-side baseline (that is a real ratchet run).
    bootstrapping = (
        args.save_baseline is not None
        and pathlib.Path(args.save_baseline) == pathlib.Path(args.baseline)
        and not baseline
    )
    if bootstrapping:
        print(
            "==> Bootstrap: --baseline and --save-baseline are the same path and it has no "
            "entries yet; creating the baseline snapshot (the ratchet comparison is skipped)."
        )
        baseline_problem = None
    else:
        baseline_problem = require_baseline(args.baseline, baseline)
    if baseline_problem:
        print(f"Coverage ratchet cannot run: {baseline_problem}", file=sys.stderr)
        return 2

    # A --core-files that is missing or entirely stale used to degrade to "no
    # core paths", i.e. the core threshold silently checked nothing.
    if args.core_files is not None and not args.core_files.exists():
        print(f"Core-file list not found: {args.core_files}", file=sys.stderr)
        return 2
    core_prefixes = load_prefix_list(args.core_files)
    if args.core_files is not None and not core_prefixes:
        print(f"Core-file list is empty: {args.core_files}", file=sys.stderr)
        return 2
    stale = stale_prefixes(core_prefixes)
    if stale:
        print(
            "Stale core-file prefixes (match no file on disk):\n  "
            + "\n  ".join(stale),
            file=sys.stderr,
        )
        return 2
    if core_prefixes:
        matched = sum(
            1 for n in _normalized_source_paths() if _matches_prefix(n, core_prefixes)
        )
        print(f"Core-path guard: {len(core_prefixes)} prefixes match {matched} files.")

    # `--non-unit-coverable` gets the same fail-closed treatment as
    # `--core-files`: a missing list, an empty list, or a prefix matching no file
    # all mean the exemption silently stopped applying (or applies to nothing),
    # which is exactly the state nobody notices.
    if args.non_unit_coverable is not None and not args.non_unit_coverable.exists():
        print(
            f"Non-unit-coverable list not found: {args.non_unit_coverable}",
            file=sys.stderr,
        )
        return 2
    non_unit_prefixes = load_prefix_list(args.non_unit_coverable)
    if args.non_unit_coverable is not None and not non_unit_prefixes:
        print(
            f"Non-unit-coverable list is empty: {args.non_unit_coverable}",
            file=sys.stderr,
        )
        return 2
    stale_non_unit = stale_prefixes(non_unit_prefixes)
    if stale_non_unit:
        print(
            "Stale non-unit-coverable prefixes (match no file on disk):\n  "
            + "\n  ".join(stale_non_unit),
            file=sys.stderr,
        )
        return 2
    if non_unit_prefixes:
        matched = sum(
            1
            for n in _normalized_source_paths()
            if _matches_prefix(n, non_unit_prefixes)
        )
        print(
            f"Non-unit-coverable guard: {len(non_unit_prefixes)} prefixes match {matched} files."
        )

    if not current:
        print("No source-file coverage data found in report.", file=sys.stderr)
        return 1

    # The raise ledger is read before the ratchet runs so a malformed ledger is
    # reported as exit 2 (unusable input), not confused with a coverage failure.
    raise_entries: List[dict] = []
    raise_closed: Optional[dict] = None
    if args.raise_queue is not None:
        try:
            raise_entries, raise_closed = load_raise_queue(args.raise_queue)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"Coverage raise queue is unusable: {error}", file=sys.stderr)
            return 2

    # `--allow-lower` 只对**基线已知**的路径有意义。打错一个字符会让它变成
    # 静默空操作（用户以为自己放行了，实际没有），所以这里 fail-closed。
    #
    # ⚠️ 路径口径是 `_normalize_path` 之后的 key（`synapse-common/error.rs`）——
    # 注意**没有** `src/` 段，与 lcov 的 `SF:` 行写法不同。这条差异是真实陷阱：
    # 拿 lcov 里的写法来点名会匹配不上，所以报错时直接把候选打出来，别让人猜。
    unknown_lower = [p for p in args.allow_lower if p not in baseline]
    if unknown_lower:
        print(
            "--allow-lower names path(s) with no baseline entry:\n  "
            + "\n  ".join(unknown_lower),
            file=sys.stderr,
        )
        for bad in unknown_lower:
            leaf = bad.rsplit("/", 1)[-1]
            candidates = sorted(p for p in baseline if p.rsplit("/", 1)[-1] == leaf)
            if candidates:
                print(
                    "  did you mean (baseline keys are normalized: `crate/<path under src/>`):\n    "
                    + "\n    ".join(candidates),
                    file=sys.stderr,
                )
        print(
            "  A new file is governed by the ramp-up/core floors, not by a recorded "
            "floor; a typo would otherwise be a silent no-op.",
            file=sys.stderr,
        )
        return 2

    exit_code = check_file_coverage(
        current=current,
        baseline=baseline,
        global_threshold=args.global_floor,
        new_file_threshold=args.new_file_floor,
        core_prefixes=core_prefixes,
        core_threshold=args.core_threshold,
        non_unit_prefixes=non_unit_prefixes,
        report_path=args.report,
        allow_lower=args.allow_lower,
        lines_found=lines_found,
        core_targets=baseline_targets,
    )

    # 打印在 save_baseline 之前：本轮收紧后 `[RAISE-DONE]` 的条目会在下一轮变成
    # 「基线已达标」，那时守卫（check_coverage_raise_queue.py）就会要求删条目。
    report_raise_queue(current, raise_entries, raise_closed)

    save_path = args.save_baseline or args.baseline
    lowered_count = save_baseline(
        save_path,
        current,
        previous=baseline,
        allow_lower=args.allow_lower,
        lines_found=lines_found,
        previous_lines=baseline_lines,
        targets=baseline_targets,
        core_prefixes=core_prefixes,
    )
    # 把"这次报告里没有、但基线保留下来"的条目数打出来：不打印的话，一份不完整的 lcov
    # 会**静默**让基线缩水（曾经的故障形态，见 `save_baseline` 的 docstring）。
    retained = len(set(baseline) - set(current))
    if retained:
        print(
            f"baseline {save_path}: {len(current)} 个文件本次测量，"
            f"{retained} 个条目保留自上次（未出现在本次报告里，不删）"
        )
    if args.allow_lower:
        print(
            f"baseline {save_path}: {lowered_count} 个 floor 被 --allow-lower 下调"
            f"（共点名 {len(args.allow_lower)} 个路径；其余仍按 max(prev, cur) 收紧）"
        )

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
