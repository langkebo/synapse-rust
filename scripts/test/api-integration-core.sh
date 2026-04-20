#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_INTEGRATION_PROFILE=core exec bash "$SCRIPT_DIR/api-integration_test.sh" "$@"
