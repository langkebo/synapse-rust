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
require_cmd rustfmt

cd "$ROOT_DIR"

bash scripts/quality/rustfmt_all.sh

# First pass lets auto-fixable hooks rewrite files in place.
pre-commit run --all-files || true

# Second pass verifies the tree is now clean.
pre-commit run --all-files --show-diff-on-failure
