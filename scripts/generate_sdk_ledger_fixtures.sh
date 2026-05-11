#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${ROOT_DIR}/tests/unit/fixtures/ledger_export_sdk"
TIMESTAMP="${TIMESTAMP:-2026-05-11T00:00:00Z}"
COMMIT="${COMMIT:-0000000000000000000000000000000000000000}"

mkdir -p "${OUTPUT_DIR}"

for profile in default worker openclaw all; do
    cargo run --features all-extensions --bin synapse_ledger_export -- \
        --profile="${profile}" \
        --timestamp="${TIMESTAMP}" \
        --commit="${COMMIT}" \
        --output="${OUTPUT_DIR}/${profile}.json"
done

printf 'sdk ledger fixtures written to %s\n' "${OUTPUT_DIR}"
