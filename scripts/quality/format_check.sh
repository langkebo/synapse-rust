#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
USER_BIN_DIR="$(python3 -m site --user-base 2>/dev/null)/bin"

export PATH="$USER_BIN_DIR:$HOME/.local/bin:$HOME/Library/Python/3.11/bin:$PATH"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

require_cmd pre-commit
require_cmd ruff
require_cmd shfmt
require_cmd python3
require_cmd rustfmt

cd "$ROOT_DIR"

bash scripts/quality/rustfmt_all.sh --check
ruff format --check .

shell_files=()
while IFS= read -r file; do
    shell_files+=("$file")
done < <(git ls-files "*.sh")

if [ "${#shell_files[@]}" -gt 0 ]; then
    shfmt -d -i 4 -ci "${shell_files[@]}"
fi

python3 scripts/quality/format_audit.py --fail-on-drift

pre-commit run check-json --all-files --show-diff-on-failure
pre-commit run check-toml --all-files --show-diff-on-failure
pre-commit run check-yaml --all-files --show-diff-on-failure
