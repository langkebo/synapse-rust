#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT_DIR"

echo "[1/5] cargo check --all-targets"
cargo check --all-targets

echo "[2/5] cargo test search_routes"
cargo test search_routes

echo "[3/5] cargo test legacy_thread"
cargo test legacy_thread

echo "[4/5] cargo test parse_dm_users_requires_string_array"
cargo test parse_dm_users_requires_string_array

echo "[5/5] verify docs/API-OPTION has no open-task markers"
python3 - <<'PY'
from pathlib import Path
import re
root = Path("docs/API-OPTION")
pattern = re.compile(r"未完成|待完成|TODO|todo|待处理")
matches = []
for path in root.glob("*.md"):
    text = path.read_text(encoding="utf-8")
    if pattern.search(text):
        matches.append(str(path))
if matches:
    raise SystemExit("Found open-task markers:\n" + "\n".join(matches))
print("No open-task markers found in top-level docs/API-OPTION markdown files")
PY

echo "Verification complete."
