#!/usr/bin/env bash
#
# CI gate: supply-chain hardening via cargo-deny + cargo-audit.
#
# This script is the single entry point that every PR must pass. It
# combines:
#   - `cargo-deny check`        (advisories + bans + licenses + sources)
#   - `cargo-audit --deny warnings` (RustSec database, JSON output)
#
# The two tools overlap on advisories, but cargo-deny is stricter on
# license/source policy and cargo-audit is the canonical RustSec
# consumer. Running both means a new CVE cannot slip in through a
# configuration mistake in either tool.
#
# Exits 0 on success, 1 on any violation.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# Use a stable advisory database snapshot so CI runs are reproducible
# (otherwise an advisory added today could break a PR that ran yesterday).
export RUSTSEC_ADVISORY_DB_URL="${RUSTSEC_ADVISORY_DB_URL:-https://github.com/rustsec/advisory-db}"
export RUSTSEC_ADVISORY_DB_BRANCH="${RUSTSEC_ADVISORY_DB_BRANCH:-main}"

mkdir -p artifacts

# -----------------------------------------------------------------------------
# 1) cargo-deny
# -----------------------------------------------------------------------------
# Installed via `cargo install --locked cargo-deny`. The script will skip
# cargo-deny with a warning if the binary is not present so that the
# workflow can be enabled incrementally on a runner image that does not
# yet have it pre-installed.
if command -v cargo-deny >/dev/null 2>&1; then
    echo "==> cargo-deny check"
    # Build flags compatible with the installed cargo-deny version.
    # 0.16+: --hide-inclusion-graph
    # 0.20+: --hide-spans, --show-stats, --format
    DENY_FLAGS=""
    if cargo deny check --help 2>&1 | grep -q hide-inclusion-graph; then
        DENY_FLAGS="${DENY_FLAGS} --hide-inclusion-graph"
    fi
    if cargo deny check --help 2>&1 | grep -q hide-spans; then
        DENY_FLAGS="${DENY_FLAGS} --hide-spans"
    fi
    if cargo deny check --help 2>&1 | grep -q show-stats; then
        DENY_FLAGS="${DENY_FLAGS} --show-stats"
    fi
    if cargo deny check --help 2>&1 | grep -q '\--format'; then
        DENY_FLAGS="${DENY_FLAGS} --format human"
    fi
    cargo deny check ${DENY_FLAGS} 2>&1 | tee artifacts/cargo-deny.txt
    if [[ ${PIPESTATUS[0]} -ne 0 ]]; then
        echo "supply_chain_gate: cargo-deny FAILED" >&2
        echo "  See artifacts/cargo-deny.txt for the full report." >&2
        exit 1
    fi
else
    echo "supply_chain_gate: cargo-deny not installed, skipping (install via \`cargo install --locked cargo-deny\`)" >&2
fi

# -----------------------------------------------------------------------------
# 2) cargo-audit
# -----------------------------------------------------------------------------
# Strict by default: any advisory (including the watched-but-not-ignored
# ones) produces a non-zero exit. The `.cargo/audit.toml` ignore list is
# the only place that should be edited to silence a finding; do not add
# `--no-fetch` here — the audit DB must be fresh on every run.
if command -v cargo-audit >/dev/null 2>&1; then
    echo "==> cargo-audit"
    cargo audit \
        --deny warnings \
        --deny unsound \
        --deny yanked \
        --json 2>&1 | tee artifacts/cargo-audit.json
    audit_status=${PIPESTATUS[0]}

    # Render a human-readable summary alongside the JSON for log readability.
    if command -v jq >/dev/null 2>&1; then
        jq -r '
            "vulnerabilities.found: " + (.vulnerabilities.found // 0 | tostring),
            "vulnerabilities.count:  " + (.vulnerabilities.count  // 0 | tostring),
            "warnings:               " + ((.warnings.list // []) | length | tostring)
        ' artifacts/cargo-audit.json
    fi

    if [[ ${audit_status} -ne 0 ]]; then
        echo "supply_chain_gate: cargo-audit FAILED (exit ${audit_status})" >&2
        echo "  See artifacts/cargo-audit.json for the full report." >&2
        exit 1
    fi
else
    echo "supply_chain_gate: cargo-audit not installed, skipping (install via \`cargo install --locked cargo-audit\`)" >&2
fi

echo "supply_chain_gate: OK"
exit 0
