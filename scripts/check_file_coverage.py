#!/usr/bin/env python3
"""Per-file coverage ratchet for lcov / tarpaulin JSON reports.

Reads a coverage report, compares per-file line coverage against the committed
baseline, and exits non-zero when a file falls below its floor.

Policy (revised 2026-09-19; see docs/audit/GATE_INTEGRITY_SWEEP_2026-09-19.md §6):
  - A file the baseline already records: **must not regress** below its recorded
    value. Absolute floors are NOT re-applied to it.
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

Why the baseline-known rule is "no regression" and not `max(prev, global)`:
the previous floor made a freshly bootstrapped baseline unsatisfiable by
construction. Measured 2026-09-19 immediately after committing the first
baseline: exit 1 with 188 files red, every one of them reporting
`was X%, delta=+0.0%` — i.e. not a regression, just an absolute floor the repo
has never met. A ratchet that is red the moment it starts working teaches people
to ignore it (`AGENTS.md` rule 8's converse).

Usage:
  python3 scripts/check_file_coverage.py \\
      --report coverage/lcov.info --format lcov \\
      --baseline scripts/ci/coverage_baseline.json \\
      --global-floor 40 --new-file-floor 30 \\
      --core-files scripts/ci/core_file_coverage_prefixes.txt \\
      --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt
"""

import argparse
import functools
import json
import pathlib
import sys
from typing import Dict, List, Optional, Tuple

ROOT = pathlib.Path(__file__).resolve().parents[1]

# Path fragments that mark a source file as test-only support code. Compared
# against the *normalized* path the report yields (e.g.
# `synapse-storage/test_mocks/room.rs`, `synapse-test-utils/lib.rs`).
TEST_ONLY_MARKERS: Tuple[str, ...] = (
    "test_mocks/",
    "synapse-test-utils/",
    "/test_utils.rs",
    "/test_isolation.rs",
    "/test_schema_guard.rs",
    "scripts/bench_harness.rs",
)


def is_test_only(path: str) -> bool:
    """True for test-support sources the coverage ratchet must ignore."""
    normalized = path if path.startswith("/") else "/" + path
    return any(marker in normalized for marker in TEST_ONLY_MARKERS)


def load_baseline(path: pathlib.Path) -> Dict[str, float]:
    """Load prior coverage baseline (file_path -> line_pct).

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
            "      --format lcov --baseline " + str(path) + " --save-baseline " + str(path) + " \\\n"
            "      --threshold 0 --global-floor 0 --new-file-floor 0 --core-threshold 0"
        )
    if not baseline:
        return (
            f"coverage baseline is empty: {path}\n"
            "  An empty baseline makes every file 'new', which disables the\n"
            "  regression check. Re-bootstrap it (see the command above)."
        )
    return None


def save_baseline(
    path: pathlib.Path, files: Dict[str, float], previous: Optional[Dict[str, float]] = None
) -> None:
    """Save the coverage snapshot, never LOWERING a recorded floor.

    A ratchet must be monotone. The previous version overwrote every entry with
    the current number, so a file whose coverage dropped had its floor lowered to
    the new, worse value — the next run would then accept the regression. With
    `previous`, each entry keeps `max(previous, current)`, so the baseline can
    only tighten. Deliberately lowering a floor is an explicit edit of this file.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    previous = previous or {}
    merged = {
        p: max(float(previous.get(p, 0.0)), float(v)) for p, v in files.items()
    }
    payload = {
        "files": [
            {"path": p, "line_pct": round(v, 2)} for p, v in sorted(merged.items())
        ]
    }
    with open(path, "w") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")


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
    """Normalized paths of every .rs file in the repo (excluding build/vendor)."""
    skip = {"target", "vendor", ".git", ".claude", "node_modules"}
    out: List[str] = []
    for path in ROOT.rglob("*.rs"):
        if any(part in skip for part in path.parts):
            continue
        out.append(_normalize_path(str(path)))
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


def parse_tarpaulin_json(report_path: pathlib.Path) -> Dict[str, float]:
    """Parse a tarpaulin JSON report and return {rel_path: line_pct}.

    Handles both the 'files' array format and the 'coverage' map format.
    """
    with open(report_path) as f:
        data = json.load(f)

    result: Dict[str, float] = {}

    # Format: { "files": [ { "path": "...", "coverage": [...] }, ... ] }
    if "files" in data:
        for entry in data["files"]:
            rel = _normalize_path(entry.get("path", ""))
            if not rel or not _is_src_rs(rel):
                continue
            cov = entry.get("coverage", [])
            result[rel] = _compute_line_pct(cov)
        return result

    # Format: { "path": coverage_array, ... } (flat map)
    for path, cov in data.items():
        rel = _normalize_path(path)
        if not rel or not _is_src_rs(rel):
            continue
        if isinstance(cov, list):
            result[rel] = _compute_line_pct(cov)
        elif isinstance(cov, (int, float)):
            result[rel] = float(cov)

    return result


def parse_lcov(report_path: pathlib.Path) -> Dict[str, float]:
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
        rel = p[len(root) + 1:]
    elif p.startswith("/"):
        # Absolute but outside the repo (should not happen for this repo's own
        # coverage reports). Return the full path: it is unique by construction,
        # so it can never silently overwrite another file's entry.
        return p.lstrip("/")
    else:
        # Already repo-relative (`tarpaulin`/`lcov` differ from `Path.rglob`,
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
            return f"{crate}/{rel[idx + len(marker):]}"
    return rel


def _is_src_rs(rel: str) -> bool:
    """Only enforce thresholds on Rust source files under src/."""
    return rel.endswith(".rs") and not rel.startswith("tests/")


def _compute_line_pct(coverage: list) -> float:
    """Given tarpaulin's per-line counts, return line coverage percentage."""
    if not coverage:
        return 0.0
    covered = sum(1 for entry in coverage if _count(entry) > 0)
    total = len(coverage)
    return (covered / total) * 100.0 if total > 0 else 0.0


def _count(entry) -> int:
    """Extract the hit count from a tarpaulin coverage entry."""
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
) -> int:
    """Enforce per-file coverage thresholds.  Returns exit code.

    Priority (highest wins): baseline-known (no regression) > core > new >
    non-unit-coverable exemption.

    The exemption is checked **after** the baseline-known branch on purpose: a
    file the coverage leg cannot execute is still forbidden from regressing below
    its recorded value, so the list can never hide a regression.
    """
    failures: List[str] = []
    warnings: List[str] = []
    core_failures: List[str] = []
    exemptions: List[str] = []
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

        if not is_new:
            # A file the baseline already knows: only regression matters. See the
            # module docstring for why `max(prev, global_threshold)` was wrong.
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
            msg = (
                f"[{tag}] {path}: {cur:.1f}% < {floor:.0f}% "
                f"(was {prev_txt}, delta={delta:+.1f}%)"
            )
            if tag == "CORE":
                core_failures.append(msg)
            else:
                failures.append(msg)
        elif is_new and cur < global_threshold:
            warnings.append(
                f"[{tag}] {path}: {cur:.1f}% (below global {global_threshold:.0f}% "
                f"but above new-file ramp-up {new_file_threshold:.0f}%)"
            )

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
            f"Thresholds: baseline-known files must not regress; "
            f"new files: core ≥{core_threshold:.0f}%, "
            f"otherwise ≥{new_file_threshold:.0f}% (ramp-up); "
            f"global ≥{global_threshold:.0f}% is a warning only."
        )
        return 1

    print(
        f"All {len(current)} source files meet coverage thresholds "
        f"(baseline-known: no regression; new: core≥{core_threshold:.0f}%, "
        f"otherwise≥{new_file_threshold:.0f}%; "
        f"non-unit-coverable exempted: {len(exemptions)})."
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Enforce per-file coverage thresholds from a tarpaulin JSON or lcov report."
    )
    parser.add_argument(
        "--report",
        required=True,
        type=pathlib.Path,
        help="Path to coverage report (tarpaulin JSON or lcov.info).",
    )
    parser.add_argument(
        "--format",
        choices=["tarpaulin", "lcov"],
        default="tarpaulin",
        help="Report format to parse (default: tarpaulin).",
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
        help="Core-path coverage floor (default: 70).",
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
        "--save-baseline",
        type=pathlib.Path,
        default=None,
        help="Path to write the updated baseline snapshot (default: overwrite --baseline).",
    )
    args = parser.parse_args()

    if not args.report.exists():
        print(f"Coverage report not found: {args.report}", file=sys.stderr)
        return 1

    if args.format == "lcov":
        current = parse_lcov(args.report)
    else:
        current = parse_tarpaulin_json(args.report)
    baseline = load_baseline(args.baseline)

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
            "Stale core-file prefixes (match no file on disk):\n  " + "\n  ".join(stale),
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
        print(f"Non-unit-coverable list not found: {args.non_unit_coverable}", file=sys.stderr)
        return 2
    non_unit_prefixes = load_prefix_list(args.non_unit_coverable)
    if args.non_unit_coverable is not None and not non_unit_prefixes:
        print(f"Non-unit-coverable list is empty: {args.non_unit_coverable}", file=sys.stderr)
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
            1 for n in _normalized_source_paths() if _matches_prefix(n, non_unit_prefixes)
        )
        print(
            f"Non-unit-coverable guard: {len(non_unit_prefixes)} prefixes match {matched} files."
        )

    if not current:
        print("No source-file coverage data found in report.", file=sys.stderr)
        return 1

    exit_code = check_file_coverage(
        current=current,
        baseline=baseline,
        global_threshold=args.global_floor,
        new_file_threshold=args.new_file_floor,
        core_prefixes=core_prefixes,
        core_threshold=args.core_threshold,
        non_unit_prefixes=non_unit_prefixes,
        report_path=args.report,
    )

    save_path = args.save_baseline or args.baseline
    save_baseline(save_path, current, previous=baseline)

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
