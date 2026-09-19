#!/usr/bin/env bash
#
# check_route_contract.sh — Route Contract Drift Gate (CI / local)
#
# Regenerates docs/synapse-rust/ROUTE_CONTRACT.md from the real route surface
# (synapse-web/src/routes/**) and fails if the committed doc has drifted from source.
#
# Why normalize? The generated doc embeds a volatile "自动生成于 <date>" line that
# changes every day. We compare a normalized form (date line + trailing whitespace
# stripped) so the gate only flags *structural* drift, never the timestamp.
#
# Exit codes:
#   0  doc is up to date (or only the timestamp changed)
#   1  structural drift detected -> regenerate & commit
#
# Usage:
#   bash scripts/contract/check_route_contract.sh
#   make route-contract-check
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

DOC="docs/synapse-rust/ROUTE_CONTRACT.md"

# Guard the extractor itself before trusting its output. Runs the real checks
# plus a mutation pass that reinstates the historical S-13 defects and requires
# the suite to go red — a guard that cannot fail is not a guard (铁律 8).
echo "==> Guard tests for extract_registered.py ..."
python3 scripts/contract/test_extract_registered.py --mutation-check

# A `:param` / `*wildcard` path literal panics `create_router` at startup under
# axum 0.8, and neither `cargo check` nor the extractor notices it. Checked
# before extraction so a path-syntax regression fails with a clear message
# instead of surfacing as an unrelated entry-count drift below.
echo "==> axum path syntax (scripts/ci/check_axum_path_syntax.py) ..."
python3 scripts/ci/check_axum_path_syntax.py

echo "==> Regenerating route surface (extract_registered.py) ..."
# EXTRACT_STRICT turns the extractor's own self-check into a hard gate:
#   * every route declared by a `*_route_manifest()` must be derived
#   * every route in the authoritative ledger_export fixtures must be derived
#   * no NEW unresolved parser construct may appear (ratchet, see
#     extract_unresolved_allowlist.txt)
# Without it, a parser regression would silently emit a *shorter* doc and the
# drift gate below would happily accept it — which is precisely how S-13
# (chained `.route(p, get().put().delete())` reporting only GET) survived.
EXTRACT_STRICT=1 python3 scripts/contract/extract_registered.py

echo "==> Regenerating ${DOC} (gen_contract_doc.py) ..."
python3 scripts/contract/gen_contract_doc.py

echo "==> Drift gate for derived_routes.rs (gen_derived_routes.py) ..."
python3 scripts/contract/gen_derived_routes.py --check

# B2-4b: the extractor guards "后端已服务 ⊆ 已登记"; this guards the OPPOSITE
# direction — "SDK 真实调用的 ⊆ 后端已登记" (后端别欠 SDK 端点). It reads the SDK
# manager source literals, NOT the generated route-table (which is "只增不减" and
# would silently pass after a prefix removal).
#
# Needs the matrix-js-sdk sibling checkout. CI checks out only this repo, so there
# the site-by-site comparison is SKIPPED — but the predicate self-test and the
# allowlist hygiene check still run, because they don't need the SDK. The SKIPPED
# banner says so explicitly so nobody mistakes it for a pass.
echo "==> SDK ⊆ ledger coverage (B2-4b, scripts/contract/check_sdk_route_coverage.py) ..."
# Surface the uncovered direction where a human will actually see it: a SKIPPED
# line buried in a long step log reads as a pass (sweep A12 / E5 — that direction
# had never run anywhere, because no caller ever set SDK_CONTRACT_STRICT). GitHub
# renders `::warning::` in the checks summary. The strict run lives in
# `e2ee-interop.yml`, the one lane that checks out matrix-js-sdk.
if [ ! -d "${SDK_ROOT:-$PWD/../matrix-js-sdk}/src" ] && [ "${GITHUB_ACTIONS:-false}" = "true" ]; then
    echo "::warning title=SDK⊆ledger not verified::matrix-js-sdk is not checked out in this lane, so the SDK⊆ledger direction was SKIPPED (not passed). It runs strictly in e2ee-interop.yml."
fi
python3 scripts/contract/check_sdk_route_coverage.py

if git diff --quiet -- "$DOC"; then
    echo "✅ ROUTE_CONTRACT.md is up to date with the source route surface."
    exit 0
fi

# Working tree differs from HEAD. Normalize away the volatile generated-date line
# and trailing whitespace, then compare. Only a real structural change fails.
python3 - "$DOC" <<'PY'
import sys, re, subprocess, difflib

doc = sys.argv[1]
new = open(doc, encoding="utf-8").read()
try:
    old = subprocess.check_output(["git", "show", f"HEAD:{doc}"], text=True)
except subprocess.CalledProcessError:
    old = ""  # untracked file: any content counts as "new"

def norm(s):
    return "\n".join(
        ln.rstrip() for ln in s.splitlines() if not re.search(r"自动生成于", ln)
    )

o, n = norm(old), norm(new)
if o == n:
    # Only the generation timestamp changed -> acceptable, do not fail.
    print("✅ ROUTE_CONTRACT.md: only the generation timestamp changed (acceptable drift).")
    sys.exit(0)

print("❌ ROUTE_CONTRACT.md has DRIFTED from the source route surface.")
print("   Regenerate and commit it:")
print("     python3 scripts/contract/extract_registered.py \\")
print("     && python3 scripts/contract/gen_contract_doc.py")
print("   Then: git add docs/synapse-rust/ROUTE_CONTRACT.md && git commit")
print("--- normalized diff (timestamp-stripped; real structural changes) ---")
for l in difflib.unified_diff(o.splitlines(), n.splitlines(), lineterm=""):
    print(l)
sys.exit(1)
PY
