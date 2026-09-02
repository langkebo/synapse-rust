#!/bin/bash
# setup_accounts.sh — Create admin and guest test accounts for T1 tests.
#
# - admin:   created via /_synapse/admin/v1/register (admin_registration API)
#   The admin endpoint is localhost-only, so we exec into the synapse-rust
#   container and call it from there. HMAC is computed locally with python3.
# - guest:   created via /_matrix/client/v3/register?kind=guest
#
# Output: writes results/creds.env with TOKEN_E2ETEST1, TOKEN_E2ETEST2,
#         TOKEN_E2EADMIN, TOKEN_E2EGUEST, USER_E2EADMIN, USER_E2EGUEST.
#
# Pre-existing accounts @e2etest1 / @e2etest2 must already exist (created by
# prior test setup). If admin/guest already exist, we log in instead of
# re-registering.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

CREDS_FILE="$RESULTS_DIR/creds.env"
: >"$CREDS_FILE"

echo "=== setup_accounts.sh — creating/loading test accounts ==="

# ---------------------------------------------------------------------------
# 1. Login e2etest1 / e2etest2 (must already exist)
# ---------------------------------------------------------------------------
TOKEN_E2ETEST1=$(login_user "e2etest1" "Test@1234")
TOKEN_E2ETEST2=$(login_user "e2etest2" "Test@1234")
if [ -z "$TOKEN_E2ETEST1" ] || [ -z "$TOKEN_E2ETEST2" ]; then
    echo "ERROR: e2etest1/e2etest2 login failed — are the accounts pre-created?"
    exit 1
fi
echo "✅ e2etest1 token acquired"
echo "✅ e2etest2 token acquired"

# ---------------------------------------------------------------------------
# 2. Create admin account via admin_registration API (localhost-only)
# ---------------------------------------------------------------------------
ADMIN_USERNAME="e2eadmin"
ADMIN_PASSWORD="Test@1234"
ADMIN_SHARED_SECRET="${ADMIN_SHARED_SECRET:-dev_admin_secret_dev_admin_secret_dev_01}"

# Try login first — if account already exists, skip registration
TOKEN_E2EADMIN=$(login_user "$ADMIN_USERNAME" "$ADMIN_PASSWORD")
if [ -n "$TOKEN_E2EADMIN" ]; then
    echo "✅ e2eadmin already exists — using existing token"
    USER_E2EADMIN="@$ADMIN_USERNAME:matrix.test"
else
    echo "→ e2eadmin not found, registering via admin_registration API..."
    # 2a. Get nonce from inside the container (localhost-only endpoint)
    NONCE_RESP=$(http_local GET "/_synapse/admin/v1/register/nonce" none "")
    NONCE_STATUS=$(echo "$NONCE_RESP" | cut -f1)
    NONCE_BODY=$(echo "$NONCE_RESP" | cut -f2-)
    if [ "$NONCE_STATUS" != "200" ]; then
        echo "❌ Failed to get nonce: HTTP $NONCE_STATUS — $NONCE_BODY"
        exit 1
    fi
    NONCE=$(printf '%s' "$NONCE_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('nonce',''))" 2>/dev/null)
    if [ -z "$NONCE" ]; then
        echo "❌ Nonce missing in response: $NONCE_BODY"
        exit 1
    fi
    echo "  nonce=$NONCE"

    # 2b. Compute HMAC-SHA256 with python3
    MAC=$(
        ADMIN_SHARED_SECRET="$ADMIN_SHARED_SECRET" NONCE="$NONCE" \
            ADMIN_USERNAME="$ADMIN_USERNAME" ADMIN_PASSWORD="$ADMIN_PASSWORD" \
            python3 - <<'PY'
import hmac, hashlib, os
secret = os.environ['ADMIN_SHARED_SECRET'].encode('utf-8')
nonce = os.environ['NONCE'].encode('utf-8')
user = os.environ['ADMIN_USERNAME'].encode('utf-8')
pwd = os.environ['ADMIN_PASSWORD'].encode('utf-8')
mac = hmac.new(secret, digestmod=hashlib.sha256)
mac.update(nonce); mac.update(b'\0')
mac.update(user); mac.update(b'\0')
mac.update(pwd); mac.update(b'\0')
mac.update(b'admin\x00\x00\x00')
print(mac.hexdigest())
PY
    )
    echo "  mac=$MAC"

    # 2c. Register admin user from inside container
    REG_BODY=$(NONCE="$NONCE" MAC="$MAC" ADMIN_USERNAME="$ADMIN_USERNAME" ADMIN_PASSWORD="$ADMIN_PASSWORD" \
        python3 -c '
import json, os
print(json.dumps({
    "nonce": os.environ["NONCE"],
    "username": os.environ["ADMIN_USERNAME"],
    "password": os.environ["ADMIN_PASSWORD"],
    "admin": True,
    "mac": os.environ["MAC"],
    "displayname": "E2E Admin"
}))
')

    REG_RESP=$(http_local POST "/_synapse/admin/v1/register" none "$REG_BODY")
    REG_STATUS=$(echo "$REG_RESP" | cut -f1)
    REG_BODY_OUT=$(echo "$REG_RESP" | cut -f2-)
    if [ "$REG_STATUS" != "200" ]; then
        echo "❌ Admin registration failed: HTTP $REG_STATUS — $REG_BODY_OUT"
        exit 1
    fi
    TOKEN_E2EADMIN=$(printf '%s' "$REG_BODY_OUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('access_token',''))" 2>/dev/null)
    USER_E2EADMIN=$(printf '%s' "$REG_BODY_OUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('user_id',''))" 2>/dev/null)
    if [ -z "$TOKEN_E2EADMIN" ] || [ -z "$USER_E2EADMIN" ]; then
        echo "❌ Admin registration response missing token/user_id: $REG_BODY_OUT"
        exit 1
    fi
    echo "✅ e2eadmin registered: $USER_E2EADMIN"
fi

# ---------------------------------------------------------------------------
# 3. Create guest account via /register?kind=guest
# ---------------------------------------------------------------------------
# Try login first (if guest already exists) — but guest tokens are session-
# scoped, so a fresh guest registration is preferred.
echo "→ Registering guest account..."
GUEST_RESP=$(http POST "/_matrix/client/v3/register?kind=guest" none "{}")
GUEST_STATUS=$(echo "$GUEST_RESP" | cut -f1)
GUEST_BODY=$(echo "$GUEST_RESP" | cut -f2-)
if [ "$GUEST_STATUS" != "200" ]; then
    echo "❌ Guest registration failed: HTTP $GUEST_STATUS — $GUEST_BODY"
    exit 1
fi
TOKEN_E2EGUEST=$(printf '%s' "$GUEST_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('access_token',''))" 2>/dev/null)
USER_E2EGUEST=$(printf '%s' "$GUEST_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('user_id',''))" 2>/dev/null)
if [ -z "$TOKEN_E2EGUEST" ] || [ -z "$USER_E2EGUEST" ]; then
    echo "❌ Guest registration response missing token/user_id: $GUEST_BODY"
    exit 1
fi
echo "✅ e2eguest registered: $USER_E2EGUEST"

# ---------------------------------------------------------------------------
# 4. Write creds file
# ---------------------------------------------------------------------------
cat >"$CREDS_FILE" <<EOF
# Auto-generated by setup_accounts.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
TOKEN_E2ETEST1="$TOKEN_E2ETEST1"
TOKEN_E2ETEST2="$TOKEN_E2ETEST2"
TOKEN_E2EADMIN="$TOKEN_E2EADMIN"
TOKEN_E2EGUEST="$TOKEN_E2EGUEST"
USER_E2ETEST1="@e2etest1:matrix.test"
USER_E2ETEST2="@e2etest2:matrix.test"
USER_E2EADMIN="$USER_E2EADMIN"
USER_E2EGUEST="$USER_E2EGUEST"
EOF
echo ""
echo "=== Credentials written to $CREDS_FILE ==="
echo "USER_E2ETEST1=@e2etest1:matrix.test"
echo "USER_E2ETEST2=@e2etest2:matrix.test"
echo "USER_E2EADMIN=$USER_E2EADMIN"
echo "USER_E2EGUEST=$USER_E2EGUEST"
