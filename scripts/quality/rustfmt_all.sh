#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODE="${1:-}"

cd "$ROOT_DIR"

rust_files=()
while IFS= read -r -d '' file; do
    # Vendored/patched upstream code and build output are out of format scope.
    # `vendor/pastey` is a byte-exact copy of upstream pastey 0.2.3 with only the
    # package name changed (see the T-PASTE-PATCH note in the root Cargo.toml),
    # so reformatting it is churn against upstream with no benefit. This mirrors
    # `scripts/check_fmt_ratchet.sh`, which enumerates real source dirs and never
    # walks `vendor/` (AGENTS.md rules 1/4). Without this exclusion the
    # `Format Compliance` job reformats vendored code while the ratchet reports
    # `current=0` — two gates disagreeing about the same tree.
    case "$file" in
        vendor/* | target/*) continue ;;
    esac
    rust_files+=("$file")
done < <(git ls-files -z "*.rs")

if [ "${#rust_files[@]}" -eq 0 ]; then
    exit 0
fi

first_nonempty_line() {
    python3 - "$1" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
for line in path.read_text(encoding="utf-8").splitlines():
    stripped = line.strip()
    if stripped:
        print(stripped)
        break
PY
}

is_stdin_format_candidate() {
    local first_line
    first_line="$(first_nonempty_line "$1")"
    case "$first_line" in
        '#!'* | 'use '* | 'extern crate '*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

# Format one file through stdin, keeping rustfmt's exit status separate from the
# resulting text.
#
# The status matters because `rustfmt` is not the only thing that can fail here:
# it is a rustup shim, so every invocation re-resolves the toolchain, and a
# component install failure makes rustfmt never run at all. `rust-toolchain.toml`
# declares `rust-src`, so on a runner whose cached 1.93.0 image has that
# component stripped, the shim aborts with
#   error: failed to install component: 'rust-src', detected conflict:
#   'lib/rustlib/src/rust/library/Cargo.lock'
# (measured on main run 35498321546). Discarding the status made that case
# indistinguishable from a real diff: the redirect left an EMPTY temp file,
# `cmp` then reported "different", and the gate printed a bogus
# `@@ -1,65 +0,0 @@` "whole file deleted" diff for
# `benches/performance_membership_benchmarks.rs` — a file with no real drift
# (`cargo fmt --all -- --check` is clean). In write mode the same path would
# `mv` the empty temp file over the source, i.e. truncate it.
run_rustfmt_stdin() {
    local file="$1" tmp="$2"
    local status=0
    rustfmt --edition 2021 <"$file" >"$tmp" || status=$?
    if [ "$status" -ne 0 ]; then
        echo "ERROR: rustfmt exited $status on $ROOT_DIR/$file — toolchain/tool failure, not a formatting diff" >&2
        return 1
    fi
}

check_file() {
    local file="$1"
    if is_stdin_format_candidate "$file"; then
        local tmp
        tmp="$(mktemp)"
        if ! run_rustfmt_stdin "$file" "$tmp"; then
            rm -f "$tmp"
            return 1
        fi
        if ! cmp -s "$tmp" "$file"; then
            echo "Diff in $ROOT_DIR/$file:" >&2
            diff -u "$file" "$tmp" >&2 || true
            rm -f "$tmp"
            return 1
        fi
        rm -f "$tmp"
    else
        rustfmt --edition 2021 --check "$file"
    fi
}

write_file() {
    local file="$1"
    if is_stdin_format_candidate "$file"; then
        local tmp
        tmp="$(mktemp)"
        if ! run_rustfmt_stdin "$file" "$tmp"; then
            rm -f "$tmp"
            return 1
        fi
        if ! cmp -s "$tmp" "$file"; then
            mv "$tmp" "$file"
        else
            rm -f "$tmp"
        fi
    else
        rustfmt --edition 2021 "$file"
    fi
}

failed=0
for file in "${rust_files[@]}"; do
    if [ "$MODE" = "--check" ]; then
        check_file "$file" || failed=1
    else
        write_file "$file" || failed=1
    fi
done

exit "$failed"
