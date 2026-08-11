#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Canonical cargo-audit config (read automatically by cargo-audit >= 0.20).
CONFIG_FILE="${ROOT_DIR}/.cargo/audit.toml"

if [ ! -f "${CONFIG_FILE}" ]; then
    echo "missing .cargo/audit.toml" >&2
    exit 1
fi

# cargo-audit 0.22 reads `.cargo/audit.toml` on its own; the legacy root
# `cargo-audit.toml` (array-of-tables format) was removed on 2026-08-11.
# Do not parse/pass --ignore here — the config file is the single source of truth.
exec cargo audit "$@"
