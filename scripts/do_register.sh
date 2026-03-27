#!/bin/bash
# Admin registration script using curl

SERVER_NAME="cjystx.top"
SERVER_URL="http://localhost:8008"
SHARED_SECRET="test_admin_secret_key_for_dev_only"

echo "=== Getting nonce ==="
NONCE_RESP=$(curl -s http://localhost:8008/_synapse/admin/v1/register/nonce -H "Host: $SERVER_NAME")
echo "Nonce response: $NONCE_RESP"

NONCE=$(echo $NONCE_RESP | python3 -c "import sys,json; print(json.load(sys.stdin)['nonce'])")
echo "Nonce: $NONCE"

USERNAME="admin7"
PASSWORD="Wzc9890951!"
ADMIN=true

echo ""
echo "=== Calculating HMAC ==="

# Build message: nonce\x00username\x00password\x00admin
MESSAGE="${NONCE}"$'\x00'"${USERNAME}"$'\x00'"${PASSWORD}"$'\x00'"admin"
echo "Message: $MESSAGE"
echo "Message hex: $(echo -n "$MESSAGE" | xxd -p)"

# Calculate HMAC-SHA256
MAC=$(echo -n "$MESSAGE" | openssl dgst -sha256 -hmac "$SHARED_SECRET" | awk '{print $2}')
echo "HMAC: $MAC"

echo ""
echo "=== Registering admin user ==="

# Register
RESP=$(curl -s -X POST http://localhost:8008/_synapse/admin/v1/register \
  -H "Host: $SERVER_NAME" \
  -H "Content-Type: application/json" \
  -d "{\"nonce\":\"$NONCE\",\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"admin\":$ADMIN,\"mac\":\"$MAC\"}")

echo "Response: $RESP"

# Check if successful
if echo "$RESP" | grep -q "access_token"; then
  echo ""
  echo "=== SUCCESS! ==="
  TOKEN=$(echo $RESP | python3 -c "import sys,json; print(json.load(sys.stdin).get('access_token',''))")
  echo "Token: $TOKEN"
  echo ""
  echo "Add to api_test_full.sh:"
  echo "ADMIN_TOKEN=\"$TOKEN\""
  echo "ADMIN_USER=\"@$USERNAME:$SERVER_NAME\""
else
  echo ""
  echo "=== FAILED ==="
  echo "$RESP"
fi