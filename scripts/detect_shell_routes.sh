#!/usr/bin/env bash
# Shell Route Detection Script
# Scans Rust route files for empty {} responses that should return business data
# Usage: bash scripts/detect_shell_routes.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ROUTE_DIR="$PROJECT_ROOT/src/web/routes"
ALLOWLIST_FILE="$SCRIPT_DIR/shell_routes_allowlist.txt"

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "=================================================="
echo "Shell Route Detection"
echo "=================================================="
echo ""

# Load allowlist into a simple list
ALLOWLIST_ENTRIES=""
if [ -f "$ALLOWLIST_FILE" ]; then
    echo -e "${BLUE}Loading allowlist from: $ALLOWLIST_FILE${NC}"
    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^#.*$ ]] && continue
        [[ -z "$line" ]] && continue
        # Store in newline-separated string
        ALLOWLIST_ENTRIES="${ALLOWLIST_ENTRIES}${line}"$'\n'
    done < "$ALLOWLIST_FILE"
    ALLOWLIST_COUNT=$(echo "$ALLOWLIST_ENTRIES" | grep -c . || echo 0)
    echo -e "${BLUE}Allowlist loaded: ${ALLOWLIST_COUNT} entries${NC}"
    echo ""
fi

# Counter for shell routes found
NEW_SHELL_ROUTES=0
KNOWN_SHELL_ROUTES=0
TOTAL_FILES_CHECKED=0

# Array to store new shell routes
declare -a NEW_ROUTES

# Function to check a file for shell routes
check_file() {
    local file="$1"
    local relative_path="${file#$ROUTE_DIR/}"

    TOTAL_FILES_CHECKED=$((TOTAL_FILES_CHECKED + 1))

    # Look for patterns like: Ok(Json(json!({})))
    local matches=$(grep -n "Ok(Json(json!({})))" "$file" 2>/dev/null || true)

    if [ -n "$matches" ]; then
        echo "$matches" | while IFS=: read -r line_num line_content; do
            local entry="$relative_path:$line_num"

            # Check if entry is in allowlist
            if echo "$ALLOWLIST_ENTRIES" | grep -Fxq "$entry"; then
                # Known shell route in allowlist
                KNOWN_SHELL_ROUTES=$((KNOWN_SHELL_ROUTES + 1))
            else
                # New shell route not in allowlist
                NEW_SHELL_ROUTES=$((NEW_SHELL_ROUTES + 1))
                NEW_ROUTES+=("$entry")

                if [ ${#NEW_ROUTES[@]} -eq 1 ]; then
                    echo -e "${RED}[NEW SHELL ROUTES DETECTED]${NC}"
                    echo ""
                fi

                echo -e "${RED}  ❌ $entry${NC}"
                echo -e "     ${YELLOW}$(echo "$line_content" | xargs)${NC}"
            fi
        done
    fi
}

# Check all route files
echo "Scanning route files in: $ROUTE_DIR"
echo ""

if [ ! -d "$ROUTE_DIR" ]; then
    echo -e "${RED}Error: Route directory not found: $ROUTE_DIR${NC}"
    exit 1
fi

# Find all .rs files in routes directory
while IFS= read -r file; do
    check_file "$file"
done < <(find "$ROUTE_DIR" -name "*.rs" -type f)

echo ""
echo "=================================================="
echo "Scan Complete"
echo "=================================================="
echo ""
echo "Files checked: $TOTAL_FILES_CHECKED"
echo -e "Known shell routes (allowlisted): ${KNOWN_SHELL_ROUTES}"
echo -e "New shell routes (not allowlisted): ${NEW_SHELL_ROUTES}"
echo ""

if [ $NEW_SHELL_ROUTES -gt 0 ]; then
    echo -e "${RED}❌ FAILED: Found $NEW_SHELL_ROUTES new shell route(s)${NC}"
    echo ""
    echo "The following shell routes are not in the allowlist:"
    for route in "${NEW_ROUTES[@]}"; do
        echo "  - $route"
    done
    echo ""
    echo "Shell routes return empty {} responses instead of business data."
    echo "Please update these routes to return meaningful confirmation data:"
    echo "  - Resource IDs (room_id, user_id, etc.)"
    echo "  - Updated field values"
    echo "  - Timestamps (created_ts, updated_ts)"
    echo ""
    echo "Example fix:"
    echo "  Before: Ok(Json(json!({})))"
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
else
    echo -e "${GREEN}✅ PASSED: No new shell routes detected${NC}"
    if [ $KNOWN_SHELL_ROUTES -gt 0 ]; then
        echo -e "${YELLOW}Note: $KNOWN_SHELL_ROUTES known shell routes are tracked in allowlist${NC}"
        echo "Consider fixing these routes to improve API quality."
    fi
    echo ""
    exit 0
fi
