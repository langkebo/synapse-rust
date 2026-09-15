#!/usr/bin/env python3
"""Trait-count ratchet for the A5 "trait 收敛" work (OPTIMIZATION_EXECUTION_PLAN §B4-1).

Counts `pub trait` declarations in the workspace crates and fails when the count
INCREASES over the recorded baseline. Same one-way semantics as
`scripts/check_fmt_ratchet.sh` and `scripts/ci/check_sqlx_dynamic_ratio.sh`:
the number may only go down.

Why a ratchet instead of "delete N traits": the A5 goal is that an abstract
storage trait must earn its keep — either it has more than one production
implementation, or it is the seam a test mock injects through. A per-turn target
would just get gamed; a monotone ceiling makes every new `*StoreApi` an explicit
decision.

Baseline: scripts/ci/trait_count_baseline  (TOTAL=<n> STORE_API=<n>)

Usage:
    python3 scripts/ci/check_trait_ratchet.py           # gate (exit 1 on increase)
    python3 scripts/ci/check_trait_ratchet.py --update  # rewrite the baseline
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
BASELINE = ROOT / "scripts" / "ci" / "trait_count_baseline"

# Workspace source roots. `tests/` is deliberately NOT scanned: test-only traits
# are fixtures, not production abstraction.
ROOTS = [
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-storage/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "src",
]
# Directories that must never be walked: stale worktree copies double every count,
# and the element-web harness carries 266 MB of screenshots.
EXCLUDED = ("/target/", "/.claude/", "/tests/element-web-harness/artifacts/", "/artifacts/")

TRAIT_RE = re.compile(r"^pub trait\s+([A-Za-z0-9_]+)", re.M)


def count() -> tuple[int, int]:
    total = 0
    store_api = 0
    for root in ROOTS:
        for dirpath, _dirnames, filenames in os.walk(ROOT / root):
            if any(marker in dirpath + "/" for marker in EXCLUDED):
                continue
            for name in filenames:
                if not name.endswith(".rs"):
                    continue
                text = (Path(dirpath) / name).read_text(encoding="utf-8", errors="replace")
                names = TRAIT_RE.findall(text)
                total += len(names)
                store_api += sum(1 for n in names if n.endswith("StoreApi"))
    return total, store_api


def read_baseline() -> tuple[int, int]:
    values = {}
    for line in BASELINE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if "=" in line:
            key, _, value = line.partition("=")
            values[key.strip()] = int(value.strip())
    return values["TOTAL"], values["STORE_API"]


def main() -> int:
    total, store_api = count()
    if "--update" in sys.argv:
        BASELINE.write_text(
            "# Trait-count ratchet baseline (A5 / B4-1). See scripts/ci/check_trait_ratchet.py.\n"
            "# The numbers may only go DOWN. Bump them deliberately and say why.\n"
            f"TOTAL={total}\n"
            f"STORE_API={store_api}\n",
            encoding="utf-8",
        )
        print(f"trait_count: baseline updated to TOTAL={total} STORE_API={store_api}")
        return 0

    base_total, base_store = read_baseline()
    print(f"trait_count: TOTAL={total} (baseline {base_total})  STORE_API={store_api} (baseline {base_store})")

    failed = False
    if total > base_total:
        print(f"FAIL: `pub trait` count increased by {total - base_total} ({base_total} -> {total})", file=sys.stderr)
        failed = True
    if store_api > base_store:
        print(
            f"FAIL: `*StoreApi` count increased by {store_api - base_store} ({base_store} -> {store_api}); "
            "a new storage trait must be a real multi-impl seam or a mock seam, otherwise use the concrete type",
            file=sys.stderr,
        )
        failed = True
    if not failed:
        if total < base_total or store_api < base_store:
            print(f"OK: decreased (TOTAL {base_total} -> {total}, STORE_API {base_store} -> {store_api})")
            print("    tighten the baseline: python3 scripts/ci/check_trait_ratchet.py --update")
        else:
            print("OK: trait counts at baseline")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
