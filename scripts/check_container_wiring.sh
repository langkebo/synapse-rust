#!/usr/bin/env bash
# H-4: CI check — verify that every service module declared in
# synapse-services/src/lib.rs is actually instantiated in container.rs
# or wiring/.  This prevents dead services from accumulating silently.
#
# Usage: bash scripts/check_container_wiring.sh
# Exit codes: 0 = all services wired, 1 = unwired services found

set -euo pipefail

SERVICES_DIR="synapse-services/src"
LIB_RS="${SERVICES_DIR}/lib.rs"
CONTAINER_RS="${SERVICES_DIR}/container.rs"
WIRING_DIR="${SERVICES_DIR}/wiring"

if [[ ! -f "$LIB_RS" ]]; then
    echo "ERROR: $LIB_RS not found" >&2
    exit 1
fi

# Extract all `pub mod <name>;` declarations from lib.rs (macOS-compatible)
MODULES=$(grep -oE '^\s*pub mod [a-z_][a-z0-9_]*' "$LIB_RS" | sed 's/.*pub mod //' | sort -u)

MODULE_COUNT=$(echo "$MODULES" | wc -l | tr -d ' ')
echo "Found ${MODULE_COUNT} pub mod declarations in lib.rs"

# Collect all text from container.rs and wiring/ for searching
WIRING_TEXT=""
if [[ -f "$CONTAINER_RS" ]]; then
    WIRING_TEXT+="$(cat "$CONTAINER_RS")"$'\n'
fi
if [[ -d "$WIRING_DIR" ]]; then
    WIRING_TEXT+="$(cat "$WIRING_DIR"/*.rs 2>/dev/null)"$'\n'
fi

UNWIRED=""

for mod_name in $MODULES; do
    # Skip infrastructure/trait/domain-group modules that don't represent
    # individual services needing direct wiring
    case "$mod_name" in
        prelude | wiring | container | shutdown | test_utils | test_mocks | capability_governance)
            continue
            ;;
        # Domain group modules — their sub-services are wired individually
        account | admin | media | infra | identity | event)
            continue
            ;;
        # Trait definitions, not instantiable services
        event_broadcaster_trait | extensible_events)
            continue
            ;;
        # Module aliases / re-exports (pub use synapse_X as Y)
        auth | worker | sync | sync_helpers | user_service | presence_service | content_scanner)
            continue
            ;;
        # Non-default feature modules (review separately)
        sms_provider)
            continue
            ;;
        # Infrastructure initialization helpers
        database_initializer | e2ee_audit)
            continue
            ;;
    esac

    # Check if the module name appears in container.rs or wiring/
    if echo "$WIRING_TEXT" | grep -qE "(^|[^a-zA-Z0-9_])${mod_name}(::|\.|[^a-zA-Z0-9_])" 2>/dev/null; then
        : # Module is referenced
    else
        UNWIRED="${UNWIRED}${mod_name}"$'\n'
    fi
done

# Trim and check
UNWIRED=$(echo "$UNWIRED" | grep -v '^$' || true)

if [[ -z "$UNWIRED" ]]; then
    echo "✅ All service modules are referenced in container.rs or wiring/"
    exit 0
else
    echo "❌ The following modules are declared in lib.rs but NOT referenced in container.rs or wiring/:"
    echo "$UNWIRED" | while read -r mod; do
        echo "   - $mod"
    done
    echo ""
    echo "These may be dead services. Either:"
    echo "  1. Wire them into ServiceContainer, or"
    echo "  2. Remove the module declaration from lib.rs, or"
    echo "  3. Add the module name to the skip list in scripts/check_container_wiring.sh"
    exit 1
fi
