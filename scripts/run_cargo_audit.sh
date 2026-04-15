#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_FILE="${ROOT_DIR}/cargo-audit.toml"

if [ ! -f "${CONFIG_FILE}" ]; then
    echo "missing cargo-audit.toml" >&2
    exit 1
fi

ARGS=()
while IFS= read -r advisory_id; do
    ARGS+=(--ignore "${advisory_id}")
done <<EOF
$(sed -n 's/^id = "\(RUSTSEC-[0-9-]*\)"$/\1/p' "${CONFIG_FILE}")
EOF

exec cargo audit "${ARGS[@]}" "$@"
