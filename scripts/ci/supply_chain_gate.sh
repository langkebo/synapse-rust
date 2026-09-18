#!/usr/bin/env bash
#
# CI gate: supply-chain hardening via cargo-deny + cargo-audit.
#
# This script is the single entry point for supply-chain policy. It combines:
#   - `cargo-deny check`            (advisories + bans + licenses + sources)
#   - `cargo-audit --deny warnings` (RustSec database, JSON output)
#
# The two tools overlap on advisories, but cargo-deny is stricter on
# license/source policy and cargo-audit is the canonical RustSec consumer.
# Running both means a new CVE cannot slip in through a configuration mistake in
# either tool.
#
# Exits 0 on success, 1 on a violation, 2 when a required tool is missing.
#
# A missing tool FAILS the gate (exit 2). The previous behaviour logged a notice,
# continued to `echo "supply_chain_gate: OK"; exit 0`, and thereby let the
# `repo-sanity` job — which installs no Rust toolchain and neither tool — report
# a green "Supply-chain gate" step on every PR while checking nothing, under a
# comment claiming "both must pass for the PR to merge". A gate that silently
# skips is worse than no gate, because the workflow asserts the check happened.
#
# `SUPPLY_CHAIN_ALLOW_MISSING_TOOLS=1` is a deliberately degraded local mode: it
# still runs whatever tools ARE installed, then reports loudly that the run was
# incomplete and must not be treated as a pass.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

missing_tools=""

# -----------------------------------------------------------------------------
# 1) cargo-deny
# -----------------------------------------------------------------------------
# Build flags compatible with the installed cargo-deny version:
#   0.16+: --hide-inclusion-graph
#   0.20+: --hide-spans, --show-stats, --format
if command -v cargo-deny >/dev/null 2>&1; then
    echo "==> cargo-deny check"
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
    mkdir -p artifacts
    cargo deny check ${DENY_FLAGS} 2>&1 | tee artifacts/cargo-deny.txt
    if [[ ${PIPESTATUS[0]} -ne 0 ]]; then
        echo "supply_chain_gate: cargo-deny FAILED" >&2
        echo "  See artifacts/cargo-deny.txt for the full report." >&2
        exit 1
    fi
else
    missing_tools="${missing_tools} cargo-deny"
fi

# -----------------------------------------------------------------------------
# 2) cargo-audit
# -----------------------------------------------------------------------------
# Strict by default: any advisory (including the watched-but-not-ignored ones)
# produces a non-zero exit. The `.cargo/audit.toml` ignore list is the only place
# that should be edited to silence a finding; do not add `--no-fetch` here — the
# audit DB must be fresh on every run.
if command -v cargo-audit >/dev/null 2>&1; then
    echo "==> cargo-audit"
    mkdir -p artifacts
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
    missing_tools="${missing_tools} cargo-audit"
fi

# -----------------------------------------------------------------------------
# Verdict
# -----------------------------------------------------------------------------
if [[ -n "${missing_tools}" ]]; then
    if [[ "${SUPPLY_CHAIN_ALLOW_MISSING_TOOLS:-0}" == "1" ]]; then
        echo "!! supply_chain_gate: INCOMPLETE — missing${missing_tools}" >&2
        echo "!! SUPPLY_CHAIN_ALLOW_MISSING_TOOLS=1 suppressed the failure above." >&2
        echo "!! This is NOT a pass: the missing tool(s) ran no check at all." >&2
        echo "!! Use it only for local work; CI must install both tools." >&2
        exit 0
    fi
    echo "supply_chain_gate: FAIL — required tool(s) not installed:${missing_tools}" >&2
    echo "  Install them (the CI 'security-audit' job does):" >&2
    echo "    cargo install --locked cargo-deny" >&2
    echo "    cargo install --locked cargo-audit" >&2
    echo "  A missing tool must not read as a pass — this script is the only consumer" >&2
    echo "  of deny.toml and .cargo/audit.toml, so skipping it checks nothing." >&2
    echo "  For a deliberately degraded local run: SUPPLY_CHAIN_ALLOW_MISSING_TOOLS=1" >&2
    exit 2
fi

echo "supply_chain_gate: OK"
exit 0
