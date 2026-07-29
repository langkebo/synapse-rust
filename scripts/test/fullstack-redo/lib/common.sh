#!/bin/bash
# common.sh - Shared helpers for T1 core backend API tests.
#
# Provides:
#   - BASE / GUEST_BASE / ADMIN_SECRET constants
#   - test_case / pass / fail / skip reporting helpers
#   - login helper that produces tokens for known accounts
#   - JSON field extractor (jq-free, python3 based)
#
# Sourced by per-module scripts. The orchestrator (run_t1_all.sh) sets up
# the account credentials file before invoking module scripts.

set -uo pipefail

# ---------------------------------------------------------------------------
# Deployment constants
# ---------------------------------------------------------------------------
BASE="${BASE:-https://matrix.test}"
CONTAINER_NAME="${CONTAINER_NAME:-synapse-rust}"
INTERNAL_URL="${INTERNAL_URL:-http://localhost:8008}"  # reachable inside container

# Account credentials (populated by setup_accounts.sh, sourced on demand)
CREDS_FILE="${CREDS_FILE:-/Users/ljf/Desktop/hu_ts/synapse-rust/scripts/test/fullstack-redo/results/creds.env}"

# Results aggregation files (per-module scripts append here)
RESULTS_DIR="${RESULTS_DIR:-/Users/ljf/Desktop/hu_ts/synapse-rust/scripts/test/fullstack-redo/results}"
mkdir -p "$RESULTS_DIR"
DETAIL_FILE="$RESULTS_DIR/details.tsv"
SUMMARY_FILE="$RESULTS_DIR/summary.txt"
ISSUES_FILE="${ISSUES_FILE:-/Users/ljf/Desktop/hu_ts/docs/superpowers/plans/2026-07-25-issues.md}"

# Per-module counters
PASS=0
FAIL=0
SKIP=0

# ---------------------------------------------------------------------------
# JSON helper (no jq dependency — uses python3)
# ---------------------------------------------------------------------------
json_get() {
    # json_get <json-string> <key> [key...]
    local data="$1"; shift
    python3 -c "
import sys, json
data = sys.argv[1]
keys = sys.argv[2:]
try:
    obj = json.loads(data)
except Exception as e:
    print('', end='')
    sys.exit(0)
for k in keys:
    if isinstance(obj, list):
        try:
            idx = int(k)
            obj = obj[idx]
        except Exception:
            print('', end='')
            sys.exit(0)
    elif isinstance(obj, dict):
        obj = obj.get(k)
    else:
        print('', end='')
        sys.exit(0)
    if obj is None:
        print('', end='')
        sys.exit(0)
if isinstance(obj, (dict, list)):
    print(json.dumps(obj), end='')
else:
    print(obj, end='')
" "$data" "$@"
}

# ---------------------------------------------------------------------------
# HTTP helper: returns "<status_code>\t<response_body>"
# ---------------------------------------------------------------------------
http() {
    # http <METHOD> <PATH> [auth-token-or-none] [json-body-or-none] [extra-curl-arg...]
    local method="$1"
    local path="$2"
    local token="${3:-}"
    local body="${4:-}"
    shift 4 2>/dev/null || shift $#

    local -a args=(curl -sk -X "$method" "$BASE$path" -w "\n__HTTP_STATUS__:%{http_code}")
    if [ -n "$token" ] && [ "$token" != "none" ]; then
        args+=(-H "Authorization: Bearer $token")
    fi
    if [ -n "$body" ]; then
        args+=(-H "Content-Type: application/json" -d "$body")
    fi
    args+=("$@")

    local raw status body_out
    raw=$("${args[@]}")
    status=$(echo "$raw" | grep -oE '__HTTP_STATUS__:[0-9]+' | tail -1 | cut -d: -f2)
    body_out=$(echo "$raw" | sed 's/__HTTP_STATUS__:[0-9]*$//' )
    # strip trailing newline added by curl -w
    body_out="${body_out%$'\n'}"
    printf '%s\t%s' "${status:-000}" "$body_out"
}

# Same as http() but executed inside the synapse-rust container (for localhost-only
# endpoints like admin registration nonce/register).
http_local() {
    local method="$1"
    local path="$2"
    local token="${3:-}"
    local body="${4:-}"
    shift 4 2>/dev/null || shift $#

    local -a args=(docker exec "$CONTAINER_NAME" curl -sk -X "$method" "$INTERNAL_URL$path" -w "\n__HTTP_STATUS__:%{http_code}")
    args+=(-H "Origin: http://localhost:8008" -H "X-Forwarded-For: 127.0.0.1")
    if [ -n "$token" ] && [ "$token" != "none" ]; then
        args+=(-H "Authorization: Bearer $token")
    fi
    if [ -n "$body" ]; then
        args+=(-H "Content-Type: application/json" -d "$body")
    fi
    args+=("$@")

    local raw status body_out
    raw=$("${args[@]}")
    status=$(echo "$raw" | grep -oE '__HTTP_STATUS__:[0-9]+' | tail -1 | cut -d: -f2)
    body_out=$(echo "$raw" | sed 's/__HTTP_STATUS__:[0-9]*$//')
    body_out="${body_out%$'\n'}"
    printf '%s\t%s' "${status:-000}" "$body_out"
}

# ---------------------------------------------------------------------------
# Reporting helpers
# ---------------------------------------------------------------------------
test_case() {
    # test_case <id> <description> <expected> <actual> [actual_body]
    local id="$1" desc="$2" expected="$3" actual="$4" body="${5:-}"
    local marker="✅"
    if [ "$actual" = "$expected" ]; then
        PASS=$((PASS+1))
        marker="✅"
        printf '%s\n' "$marker $id | $desc | HTTP $actual"
    else
        FAIL=$((FAIL+1))
        marker="❌"
        local short_body
        short_body=$(printf '%s' "$body" | head -c 200)
        printf '%s\n' "$marker $id | $desc | Expected $expected, Got $actual | Body: $short_body"
    fi
    # Append a TSV row: module<TAB>id<TAB>desc<TAB>expected<TAB>actual<TAB>body
    local module_name="${MODULE:-unknown}"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$module_name" "$id" "$desc" "$expected" "$actual" "$(printf '%s' "$body" | tr '\t\n' ' ')" >> "$DETAIL_FILE"
}

skip_case() {
    local id="$1" desc="$2" reason="${3:-}"
    SKIP=$((SKIP+1))
    printf '⊘ SKIP %s | %s | %s\n' "$id" "$desc" "$reason"
    local module_name="${MODULE:-unknown}"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$module_name" "$id" "$desc" "SKIP" "SKIP" "$reason" >> "$DETAIL_FILE"
}

emit_summary() {
    local module_name="${MODULE:-unknown}"
    printf 'MODULE %s: PASS=%d FAIL=%d SKIP=%d\n' "$module_name" "$PASS" "$FAIL" "$SKIP" | tee -a "$SUMMARY_FILE"
}

# ---------------------------------------------------------------------------
# Auth helpers
# ---------------------------------------------------------------------------
login_user() {
    # login_user <username> <password>
    # echoes the access_token (empty on failure)
    local user="$1" pass="$2"
    local body resp token
    body='{"type":"m.login.password","identifier":{"type":"m.id.user","user":"'"$user"'"},"password":"'"$pass"'"}'
    resp=$(http POST "/_matrix/client/v3/login" none "$body")
    token=$(echo "$resp" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('access_token',''))" 2>/dev/null || echo "")
    printf '%s' "$token"
}

# Loads credentials created by setup_accounts.sh. Returns 0 on success.
load_creds() {
    if [ ! -f "$CREDS_FILE" ]; then
        echo "ERROR: credentials file not found at $CREDS_FILE — run setup_accounts.sh first" >&2
        return 1
    fi
    # shellcheck disable=SC1090
    source "$CREDS_FILE"
}

# Convenience getters — call load_creds() first.
get_token_e2etest1() { printf '%s' "$TOKEN_E2ETEST1"; }
get_token_e2etest2() { printf '%s' "$TOKEN_E2ETEST2"; }
get_token_e2eadmin() { printf '%s' "$TOKEN_E2EADMIN"; }
get_token_e2eguest() { printf '%s' "$TOKEN_E2EGUEST"; }

# Records a discovered issue in the issues.md appendix (no P-XXX numbering —
# the orchestrator post-processes and numbers issues after all scripts run).
record_issue() {
    # record_issue <module> <severity> <testcase_id> <description>
    local module="$1" severity="$2" tcid="$3" description="$4"
    local file="$RESULTS_DIR/discovered_issues.txt"
    printf 'MODULE=%s\tSEVERITY=%s\tTC=%s\tDESC=%s\n' "$module" "$severity" "$tcid" "$description" >> "$file"
}
