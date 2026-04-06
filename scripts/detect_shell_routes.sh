#!/usr/bin/env bash
# Shell Route Detection Script
# Scans Rust route files for empty {} success responses that should return business data.
# Usage: bash scripts/detect_shell_routes.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ROUTE_DIR="$PROJECT_ROOT/src/web/routes"
ALLOWLIST_FILE="$SCRIPT_DIR/shell_routes_allowlist.txt"

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "=================================================="
echo "Shell Route Detection"
echo "=================================================="
echo ""
echo "Scanning route files in: $ROUTE_DIR"
echo ""

if [ ! -d "$ROUTE_DIR" ]; then
    echo -e "${RED}Error: Route directory not found: $ROUTE_DIR${NC}"
    exit 1
fi

SCAN_JSON="$(python3 - "$ROUTE_DIR" "$ALLOWLIST_FILE" <<'PY'
import json
import re
import sys
from pathlib import Path

route_dir = Path(sys.argv[1])
allowlist_file = Path(sys.argv[2])

allowlist_entries = set()
if allowlist_file.exists():
    for raw_line in allowlist_file.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        allowlist_entries.add(line)

pattern = re.compile(
    r"""
    Ok
    \s*\(
        \s*
        (?:
            Json
            \s*\(
                \s*(?:serde_json::)?json!
                \s*\(
                    \s*\{\s*\}
                \s*\)
            \s*\)
          |
            empty_json
            \s*\(
                \s*
            \)
        )
    \s*\)
    """,
    re.MULTILINE | re.DOTALL | re.VERBOSE,
)

files = sorted(route_dir.rglob("*.rs"))
known = []
new = []

for file in files:
    content = file.read_text(encoding="utf-8")
    relative_path = file.relative_to(route_dir).as_posix()

    seen_lines = set()
    for match in pattern.finditer(content):
        line_no = content.count("\n", 0, match.start()) + 1
        if line_no in seen_lines:
            continue
        seen_lines.add(line_no)

        entry = f"{relative_path}:{line_no}"
        snippet = " ".join(match.group(0).split())
        record = {"entry": entry, "snippet": snippet}
        if entry in allowlist_entries:
            known.append(record)
        else:
            new.append(record)

payload = {
    "files_checked": len(files),
    "allowlist_count": len(allowlist_entries),
    "known": known,
    "new": new,
}
print(json.dumps(payload))
PY
)"

ALLOWLIST_COUNT="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["allowlist_count"])' <<<"$SCAN_JSON")"
KNOWN_SHELL_ROUTES="$(python3 -c 'import json,sys; print(len(json.loads(sys.stdin.read())["known"]))' <<<"$SCAN_JSON")"
NEW_SHELL_ROUTES="$(python3 -c 'import json,sys; print(len(json.loads(sys.stdin.read())["new"]))' <<<"$SCAN_JSON")"
TOTAL_FILES_CHECKED="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["files_checked"])' <<<"$SCAN_JSON")"

echo -e "${BLUE}Allowlist loaded: ${ALLOWLIST_COUNT} entries${NC}"
echo ""

if [ "$NEW_SHELL_ROUTES" -gt 0 ]; then
    echo -e "${RED}[NEW SHELL ROUTES DETECTED]${NC}"
    echo ""
    SCAN_JSON="$SCAN_JSON" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["SCAN_JSON"])
for item in payload["new"]:
    print(f"\033[0;31m  ❌ {item['entry']}\033[0m")
    print(f"     \033[1;33m{item['snippet']}\033[0m")
PY
    echo ""
fi

echo "=================================================="
echo "Scan Complete"
echo "=================================================="
echo ""
echo "Files checked: $TOTAL_FILES_CHECKED"
echo "Allowlisted matches (reviewed): $KNOWN_SHELL_ROUTES"
echo "New shell routes (not allowlisted): $NEW_SHELL_ROUTES"
echo ""

if [ "$NEW_SHELL_ROUTES" -gt 0 ]; then
    echo -e "${RED}❌ FAILED: Found $NEW_SHELL_ROUTES new shell route(s)${NC}"
    echo ""
    echo "The following shell routes are not in the allowlist:"
    SCAN_JSON="$SCAN_JSON" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["SCAN_JSON"])
for item in payload["new"]:
    print(f"  - {item['entry']}")
PY
    echo ""
    echo "Shell routes return empty-success responses instead of business data."
    echo "Please update these routes to return meaningful confirmation data:"
    echo "  - Resource IDs (room_id, user_id, etc.)"
    echo "  - Updated field values"
    echo "  - Timestamps (created_ts, updated_ts)"
    echo ""
    echo "Example fix:"
    echo "  Before: Ok(Json(json!({})))"
    echo "      or: Ok(empty_json())"
    echo "  After:  Ok(Json(json!({"
    echo "      \"resource_id\": id,"
    echo "      \"field\": value,"
    echo "      \"updated_ts\": chrono::Utc::now().timestamp_millis()"
    echo "  })))"
    echo ""
    echo "If this is intentional (e.g., DELETE operation), add to allowlist:"
    echo "  echo 'filename:line_number' >> scripts/shell_routes_allowlist.txt"
    echo ""
    exit 1
fi

echo -e "${GREEN}✅ PASSED: No new shell routes detected${NC}"
if [ "$KNOWN_SHELL_ROUTES" -gt 0 ]; then
    echo -e "${YELLOW}Note: $KNOWN_SHELL_ROUTES reviewed empty-success matches are tracked in allowlist${NC}"
    echo "Review them periodically: some are acceptable ACKs, others remain cleanup debt."
fi
echo ""
