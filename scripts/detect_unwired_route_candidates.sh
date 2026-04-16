#!/usr/bin/env bash
# Unwired Route Candidate Detection Script
# Scans route modules for exported handlers or router factories that are not wired.
# Usage: bash scripts/detect_unwired_route_candidates.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_DIR="$PROJECT_ROOT/src"
ROUTE_DIR="$SOURCE_DIR/web/routes"
ALLOWLIST_FILE="$SCRIPT_DIR/unwired_route_candidates_allowlist.txt"

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "=================================================="
echo "Unwired Route Candidate Detection"
echo "=================================================="
echo ""
echo "Scanning route files in: $ROUTE_DIR"
echo ""

if [ ! -d "$ROUTE_DIR" ]; then
    echo -e "${RED}Error: Route directory not found: $ROUTE_DIR${NC}"
    exit 1
fi

SCAN_JSON="$(python3 - "$ROUTE_DIR" "$SOURCE_DIR" "$ALLOWLIST_FILE" <<'PY'
import json
import re
import sys
from pathlib import Path

route_dir = Path(sys.argv[1])
source_dir = Path(sys.argv[2])
allowlist_file = Path(sys.argv[3])

allowlist_entries = set()
if allowlist_file.exists():
    for raw_line in allowlist_file.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        allowlist_entries.add(line)

all_files = sorted(route_dir.rglob("*.rs"))
all_contents = {
    file.relative_to(route_dir).as_posix(): file.read_text(encoding="utf-8")
    for file in all_files
}
source_files = sorted(source_dir.rglob("*.rs"))
source_contents = {
    file.relative_to(source_dir).as_posix(): file.read_text(encoding="utf-8")
    for file in source_files
}

handler_candidate_files = [file for file in all_files if file.name != "mod.rs"]

handler_def_re = re.compile(r"^\s*pub(?:\([^)]*\))?\s+async\s+fn\s+([A-Za-z0-9_]+)\s*\(", re.MULTILINE)
router_factory_def_re = re.compile(
    r"^\s*pub(?:\([^)]*\))?\s+fn\s+(create_[A-Za-z0-9_]*router)\s*\(",
    re.MULTILINE,
)
plain_call_template = r"\b{symbol}\s*\("
route_ref_template = (
    r"\b(?:get|post|put|delete|patch|head|options|trace|any)\s*\(\s*(?:[A-Za-z0-9_:]+::)?{symbol}\s*\)"
)


def iter_non_definition_matches(content: str, pattern: re.Pattern[str], definition_line: int):
    for match in pattern.finditer(content):
        line_no = content.count("\n", 0, match.start()) + 1
        if line_no == definition_line:
            continue
        yield line_no


def has_same_file_reference(content: str, symbol: str, definition_line: int) -> bool:
    route_ref_re = re.compile(route_ref_template.format(symbol=re.escape(symbol)))
    plain_call_re = re.compile(plain_call_template.format(symbol=re.escape(symbol)))

    if any(iter_non_definition_matches(content, route_ref_re, definition_line)):
        return True

    return any(iter_non_definition_matches(content, plain_call_re, definition_line))


def has_cross_file_route_reference(relative_path: str, symbol: str) -> bool:
    route_ref_re = re.compile(route_ref_template.format(symbol=re.escape(symbol)))
    for other_path, other_content in all_contents.items():
        if other_path == relative_path:
            continue
        if route_ref_re.search(other_content):
            return True
    return False


def has_cross_file_symbol_call(relative_path: str, symbol: str) -> bool:
    plain_call_re = re.compile(
        r"\b(?:[A-Za-z0-9_:]+::)?{symbol}\s*\(".format(symbol=re.escape(symbol))
    )
    for other_path, other_content in source_contents.items():
        if other_path == f"web/routes/{relative_path}":
            continue
        if plain_call_re.search(other_content):
            return True
    return False


known = []
new = []

for file in handler_candidate_files:
    relative_path = file.relative_to(route_dir).as_posix()
    content = all_contents[relative_path]

    for match in handler_def_re.finditer(content):
        symbol = match.group(1)
        line_no = content.count("\n", 0, match.start()) + 1
        entry = f"{relative_path}:{symbol}"

        if has_same_file_reference(content, symbol, line_no) or has_cross_file_route_reference(
            relative_path, symbol
        ):
            continue

        record = {
            "entry": entry,
            "path": relative_path,
            "line": line_no,
            "symbol": symbol,
            "kind": "handler",
        }
        if entry in allowlist_entries:
            known.append(record)
        else:
            new.append(record)

for relative_path, content in all_contents.items():
    for match in router_factory_def_re.finditer(content):
        symbol = match.group(1)
        line_no = content.count("\n", 0, match.start()) + 1
        entry = f"{relative_path}:{symbol}"

        if has_cross_file_symbol_call(relative_path, symbol):
            continue

        record = {
            "entry": entry,
            "path": relative_path,
            "line": line_no,
            "symbol": symbol,
            "kind": "router_factory",
        }
        if entry in allowlist_entries:
            known.append(record)
        else:
            new.append(record)

payload = {
    "files_checked": len(all_files),
    "allowlist_count": len(allowlist_entries),
    "known": known,
    "new": new,
}
print(json.dumps(payload))
PY
)"

ALLOWLIST_COUNT="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["allowlist_count"])' <<<"$SCAN_JSON")"
KNOWN_CANDIDATES="$(python3 -c 'import json,sys; print(len(json.loads(sys.stdin.read())["known"]))' <<<"$SCAN_JSON")"
NEW_CANDIDATES="$(python3 -c 'import json,sys; print(len(json.loads(sys.stdin.read())["new"]))' <<<"$SCAN_JSON")"
TOTAL_FILES_CHECKED="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["files_checked"])' <<<"$SCAN_JSON")"

echo -e "${BLUE}Allowlist loaded: ${ALLOWLIST_COUNT} entries${NC}"
echo ""

if [ "$NEW_CANDIDATES" -gt 0 ]; then
    echo -e "${RED}[UNWIRED ROUTE CANDIDATES DETECTED]${NC}"
    echo ""
    SCAN_JSON="$SCAN_JSON" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["SCAN_JSON"])
for item in payload["new"]:
    print(f"\033[0;31m  ❌ {item['entry']}\033[0m")
    print(f"     \033[1;33m{item['kind']} at {item['path']}:{item['line']}\033[0m")
PY
    echo ""
fi

echo "=================================================="
echo "Scan Complete"
echo "=================================================="
echo ""
echo "Files checked: $TOTAL_FILES_CHECKED"
echo "Allowlisted matches (reviewed): $KNOWN_CANDIDATES"
echo "New unwired route candidates: $NEW_CANDIDATES"
echo ""

if [ "$NEW_CANDIDATES" -gt 0 ]; then
    echo -e "${RED}❌ FAILED: Found $NEW_CANDIDATES unwired route candidate(s)${NC}"
    echo ""
    echo "The following exported route handlers/router factories are not referenced:"
    SCAN_JSON="$SCAN_JSON" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["SCAN_JSON"])
for item in payload["new"]:
    print(f"  - {item['entry']} ({item['kind']}, line {item['line']})")
PY
    echo ""
    echo "Please do one of the following:"
    echo "  1. Wire the handler/factory into the router tree"
    echo "  2. Delete the dead code and its isolated dependencies"
    echo "  3. If intentional and reviewed, add it to:"
    echo "     scripts/unwired_route_candidates_allowlist.txt"
    echo ""
    exit 1
fi

echo -e "${GREEN}✅ PASSED: No new unwired route candidates detected${NC}"
if [ "$KNOWN_CANDIDATES" -gt 0 ]; then
    echo -e "${YELLOW}Note: $KNOWN_CANDIDATES reviewed candidates are tracked in allowlist${NC}"
    echo "Review them periodically to make sure they remain intentionally unreachable."
fi
echo ""
