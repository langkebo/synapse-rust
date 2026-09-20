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

check_file() {
    local file="$1"
    if is_stdin_format_candidate "$file"; then
        local tmp
        tmp="$(mktemp)"
        rustfmt --edition 2021 <"$file" >"$tmp"
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
        rustfmt --edition 2021 <"$file" >"$tmp"
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
        write_file "$file"
    fi
done

exit "$failed"
