#!/usr/bin/env bash
#
# Spell-check one markdown file with aspell.
#
# Usage:
#   bash scripts/check_doc_spelling.sh FILE.md
#
# Exit codes (distinguishable on purpose):
#   0 = no unknown words
#   1 = unknown word(s) found (the list is printed)
#   2 = misuse / missing tool (no such file, aspell not installed)
#
# Why the distinction matters: this script used to exit 1 in ALL three cases,
# so its only CI caller — which wrapped it in
# `... || { echo "::warning::Spelling issues in $f"; }` — could never fail, and
# simply removing that wrapper would have made the step permanently red for
# clean files too. Both had to be fixed together.
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 FILE.md" >&2
    exit 2
fi

file="$1"
if [[ ! -f "$file" ]]; then
    echo "check_doc_spelling: no such file: $file" >&2
    exit 2
fi

# A missing tool must not look like a clean file.
if ! command -v aspell >/dev/null 2>&1; then
    echo "check_doc_spelling: aspell not installed (apt-get install -y aspell aspell-en)" >&2
    exit 2
fi

tmp="$(mktemp)"
filtered="$(mktemp)"
trap 'rm -f "$tmp" "$filtered"' EXIT

# `grep -Ev` exits 1 when it selects NOTHING, which is precisely the clean case
# (aspell listed no unknown words). Under `pipefail` + `set -e` that aborted the
# whole script, so a clean file reported failure — and the previous version had
# no `trap` either, leaking both temp files on every run.
#
# Only grep's "no match" status is tolerated. `aspell` itself is verified above
# and stays inside the pipeline, so a broken toolchain cannot masquerade as clean.
aspell --personal=./.aspell.en.pws --lang=en_US --mode=markdown list <"$file" |
    tr '[:upper:]' '[:lower:]' |
    sed -E 's/[^a-z].*$//' |
    sed '/^$/d' |
    { grep -Ev '^[a-f]+$' || true; } |
    sort -u >"$tmp"

if [ -f ./.aspell.ignore.txt ]; then
    grep -vxFf ./.aspell.ignore.txt "$tmp" >"$filtered" || true
else
    cp "$tmp" "$filtered"
fi

if [ -s "$filtered" ]; then
    echo "=== 未识别词汇 ==="
    cat "$filtered"
    echo ""
    echo "将上述词汇添加到 .aspell.ignore.txt（每行一个，按字母序）："
    echo "  echo '<词汇>' >> .aspell.ignore.txt && sort -u -o .aspell.ignore.txt .aspell.ignore.txt"
    echo "    （或使用：sed -i '' '1i <词汇>' .aspell.ignore.txt && sort -u -o .aspell.ignore.txt .aspell.ignore.txt）"
    exit 1
fi

exit 0
