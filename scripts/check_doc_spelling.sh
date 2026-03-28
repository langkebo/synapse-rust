#!/usr/bin/env bash
set -euo pipefail

file="${1:-docs/db/DIAGNOSIS_REPORT.md}"
tmp="$(mktemp)"
filtered="$(mktemp)"

aspell --personal=./.aspell.en.pws --lang=en_US --mode=markdown list < "$file" \
  | tr '[:upper:]' '[:lower:]' \
  | sed -E 's/[^a-z].*$//' \
  | sed '/^$/d' \
  | grep -Ev '^[a-f]+$' \
  | sort -u > "$tmp"

if [ -f ./.aspell.ignore.txt ]; then
  grep -vxFf ./.aspell.ignore.txt "$tmp" > "$filtered" || true
else
  cp "$tmp" "$filtered"
fi

if [ -s "$filtered" ]; then
  cat "$filtered"
  exit 1
fi
