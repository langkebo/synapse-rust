#!/usr/bin/env bash
# Clippy error ratchet (ROUND2-ISSUE-1, 2026-07-17)
#
# Counts current `cargo clippy --all-targets` errors and fails if the count
# exceeds the recorded baseline. The baseline starts at 3416 (the value
# reported in docs/synapse-rust/issues/ROUND2-ISSUE-1-clippy-tests-unwrap-explosion.md)
# and must only ever decrease. New clippy errors added by a PR will fail this
# gate; existing errors may be cleaned up incrementally to drive the baseline
# down.
#
# Usage:
#   bash scripts/check_clippy_ratchet.sh           # default baseline 3416
#   CLIPPY_RATCHET_BASELINE=3000 bash scripts/check_clippy_ratchet.sh
#
# Exit codes:
#   0  current error count <= baseline
#   1  current error count > baseline (regression)
#   2  clippy invocation failed
#
# See also: hula/scripts/check-ratchet.mjs (sibling ratchet pattern).

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BASELINE="${CLIPPY_RATCHET_BASELINE:-3416}"

# Run clippy with JSON output so we can count errors deterministically.
# `--message-format=json` emits one JSON object per diagnostic.
# We count entries whose `level` field is `error`.
echo "[check_clippy_ratchet] Running cargo clippy --all-targets (this may take a moment)..."
CLIPPY_OUTPUT=$(cargo clippy --all-targets --all-features --message-format=json --locked 2>/dev/null) || {
  echo "❌ cargo clippy invocation failed"
  exit 2
}

CURRENT=$(printf '%s\n' "$CLIPPY_OUTPUT" | grep -c '"level":"error"') || CURRENT=0

echo "[check_clippy_ratchet] Clippy errors: current=$CURRENT, baseline=$BASELINE"

if [ "$CURRENT" -gt "$BASELINE" ]; then
  echo "❌ Clippy error count increased ($BASELINE → $CURRENT)"
  echo "   Fix the new clippy errors or lower the baseline in scripts/check_clippy_ratchet.sh"
  echo "   after verifying the cleanup is committed."
  exit 1
fi

DELTA=$((BASELINE - CURRENT))
if [ "$DELTA" -gt 0 ]; then
  echo "✅ Clippy ratchet OK (baseline can be lowered by $DELTA: $BASELINE → $CURRENT)"
else
  echo "✅ Clippy ratchet OK (at baseline)"
fi
