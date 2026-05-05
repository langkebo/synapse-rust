#!/usr/bin/env bash
# End-to-end test of the key backup endpoints against the live homeserver.
set -euo pipefail

BASE="${BASE:-https://localhost:8448}"
CURL=(curl -sk -H 'Content-Type: application/json')

pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1"; echo "    expected: $2"; echo "    got: $3"; exit 1; }

USER="kbtest_$RANDOM$RANDOM"
PASS="P@ss$RANDOM!"

echo "=== 1. register $USER ==="
init=$("${CURL[@]}" -X POST "$BASE/_matrix/client/v3/register" -d "{\"username\":\"$USER\",\"password\":\"$PASS\",\"auth\":{\"type\":\"m.login.dummy\"}}")
echo "$init" | head -c 300; echo
SESSION=$(echo "$init" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session",""))')
TOKEN=$(echo "$init" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("access_token",""))')
USERID=$(echo "$init" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("user_id",""))')
if [[ -z "$TOKEN" ]]; then
  echo "no token in initial response, retrying with session=$SESSION"
  init=$("${CURL[@]}" -X POST "$BASE/_matrix/client/v3/register" -d "{\"username\":\"$USER\",\"password\":\"$PASS\",\"auth\":{\"type\":\"m.login.dummy\",\"session\":\"$SESSION\"}}")
  echo "$init" | head -c 400; echo
  TOKEN=$(echo "$init" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("access_token",""))')
  USERID=$(echo "$init" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("user_id",""))')
fi
[[ -n "$TOKEN" ]] || fail "register" "access_token" "$init"
pass "registered $USERID, token len=${#TOKEN}"

AUTH=(-H "Authorization: Bearer $TOKEN")

echo "=== 2. GET /room_keys/version with no backup -> 404 ==="
code=$("${CURL[@]}" "${AUTH[@]}" -o /tmp/kb_resp.json -w "%{http_code}" "$BASE/_matrix/client/v3/room_keys/version")
[[ "$code" == "404" ]] || fail "no-backup GET" "404" "$code body=$(cat /tmp/kb_resp.json)"
errcode=$(python3 -c 'import sys,json;print(json.load(open("/tmp/kb_resp.json")).get("errcode",""))')
[[ "$errcode" == "M_NOT_FOUND" ]] || fail "errcode" "M_NOT_FOUND" "$errcode"
pass "404 with M_NOT_FOUND"

echo "=== 3. POST /room_keys/version (create) ==="
create_body='{"algorithm":"m.megolm_backup.v1.curve25519-aes-sha2","auth_data":{"public_key":"abcdef","signatures":{}}}'
create=$("${CURL[@]}" "${AUTH[@]}" -X POST "$BASE/_matrix/client/v3/room_keys/version" -d "$create_body")
echo "  $create"
VER=$(echo "$create" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("version",""))')
[[ -n "$VER" ]] || fail "create" "version" "$create"
pass "created version=$VER"

echo "=== 4. GET /room_keys/version (latest) -> flat shape ==="
got=$("${CURL[@]}" "${AUTH[@]}" "$BASE/_matrix/client/v3/room_keys/version")
echo "  $got"
python3 - <<PY || fail "latest shape" "algorithm/auth_data/count/etag/version" "$got"
import json,sys
d=json.loads('''$got''')
for k in ("algorithm","auth_data","count","etag","version"):
    assert k in d, f"missing {k}"
assert d["algorithm"]=="m.megolm_backup.v1.curve25519-aes-sha2"
assert d["count"]==0
assert d["version"]=="$VER"
print("  shape ok")
PY
pass "latest GET returns spec-shaped object"

echo "=== 5. GET /room_keys/version/$VER (specific) ==="
got=$("${CURL[@]}" "${AUTH[@]}" "$BASE/_matrix/client/v3/room_keys/version/$VER")
echo "  $got"
python3 - <<PY || fail "specific shape" "algorithm/auth_data/count/etag/version" "$got"
import json
d=json.loads('''$got''')
for k in ("algorithm","auth_data","count","etag","version"):
    assert k in d, f"missing {k}"
PY
pass "specific GET returns spec-shaped object"

ROOM='!r1:matrix.test'
SESS='abcsession'

echo "=== 6. PUT /room_keys/keys/{room}/{session}?version=$VER ==="
key_body='{"first_message_index":0,"forwarded_count":0,"is_verified":true,"session_data":{"ephemeral":"e","ciphertext":"c","mac":"m"}}'
put=$("${CURL[@]}" "${AUTH[@]}" -X PUT "$BASE/_matrix/client/v3/room_keys/keys/$ROOM/$SESS?version=$VER" -d "$key_body")
echo "  $put"
pass "single-key PUT accepted"

echo "=== 7. PUT /room_keys/keys/{room}?version=$VER (room batch) ==="
batch_body='{"sessions":{"sess2":{"first_message_index":1,"forwarded_count":0,"is_verified":false,"session_data":{"ephemeral":"e2","ciphertext":"c2","mac":"m2"}}}}'
put=$("${CURL[@]}" "${AUTH[@]}" -X PUT "$BASE/_matrix/client/v3/room_keys/keys/$ROOM?version=$VER" -d "$batch_body")
echo "  $put"
pass "room batch PUT accepted"

echo "=== 8. PUT /room_keys/keys?version=$VER (all rooms batch) ==="
all_body='{"rooms":{"!r2:matrix.test":{"sessions":{"sess3":{"first_message_index":0,"forwarded_count":0,"is_verified":false,"session_data":{"ephemeral":"e3","ciphertext":"c3","mac":"m3"}}}}}}'
put=$("${CURL[@]}" "${AUTH[@]}" -X PUT "$BASE/_matrix/client/v3/room_keys/keys?version=$VER" -d "$all_body")
echo "  $put"
pass "all-rooms PUT accepted"

echo "=== 9. GET /room_keys/version (count should be > 0) ==="
got=$("${CURL[@]}" "${AUTH[@]}" "$BASE/_matrix/client/v3/room_keys/version")
echo "  $got"
cnt=$(echo "$got" | python3 -c 'import sys,json;print(json.load(sys.stdin)["count"])')
echo "  count=$cnt"
[[ "$cnt" -eq 3 ]] || fail "count after 3 uploads" "3" "$cnt"
pass "count=3 reflects all uploaded sessions across rooms"

echo "=== 10. GET /room_keys/keys?version=$VER (all rooms) ==="
got=$("${CURL[@]}" "${AUTH[@]}" "$BASE/_matrix/client/v3/room_keys/keys?version=$VER")
echo "  $got"
python3 - <<PY || fail "all-rooms shape" "{rooms:{room:{sessions:{sid:KeyBackupData}}}}" "$got"
import json
d=json.loads('''$got''')
rooms=d.get("rooms",{})
assert "$ROOM" in rooms, f"missing $ROOM"
assert "!r2:matrix.test" in rooms, f"missing !r2"
assert "abcsession" in rooms["$ROOM"]["sessions"]
assert "sess2" in rooms["$ROOM"]["sessions"]
assert "sess3" in rooms["!r2:matrix.test"]["sessions"]
PY
pass "all-rooms read returns full {rooms:{...}} tree"

echo "=== 11. GET /room_keys/keys/{room}?version=$VER ==="
got=$("${CURL[@]}" "${AUTH[@]}" "$BASE/_matrix/client/v3/room_keys/keys/$ROOM?version=$VER")
echo "  $got"
python3 - <<PY || fail "room read shape" "{sessions:{sid:{is_verified,session_data:{...}}}}" "$got"
import json
d=json.loads('''$got''')
assert "sessions" in d, "missing sessions"
assert "abcsession" in d["sessions"], "missing abcsession"
s=d["sessions"]["abcsession"]
assert s.get("is_verified") is True, f"is_verified not persisted: {s}"
sd=s.get("session_data")
assert isinstance(sd,dict) and sd.get("ciphertext")=="c", f"session_data wrapping wrong: {sd}"
print("  shape ok")
PY
pass "room read returned, is_verified preserved, session_data unwrapped"

echo "=== 12. GET /room_keys/keys/{room}/{session}?version=$VER ==="
got=$("${CURL[@]}" "${AUTH[@]}" "$BASE/_matrix/client/v3/room_keys/keys/$ROOM/$SESS?version=$VER")
echo "  $got"
python3 - <<PY || fail "session read shape" "KeyBackupData object" "$got"
import json
d=json.loads('''$got''')
assert d.get("is_verified") is True, f"is_verified not persisted: {d}"
sd=d.get("session_data")
assert isinstance(sd,dict) and sd.get("ciphertext")=="c", f"session_data wrong: {sd}"
PY
pass "single-session read returns KeyBackupData with all fields"

echo "=== 13. PUT /room_keys/version/$VER (update auth_data) ==="
upd_body='{"algorithm":"m.megolm_backup.v1.curve25519-aes-sha2","auth_data":{"public_key":"newkey","signatures":{}},"version":"'"$VER"'"}'
upd=$("${CURL[@]}" "${AUTH[@]}" -X PUT "$BASE/_matrix/client/v3/room_keys/version/$VER" -d "$upd_body")
echo "  $upd"
pass "version metadata updated"

echo "=== 14. DELETE /room_keys/keys/{room}/{session}?version=$VER ==="
del=$("${CURL[@]}" "${AUTH[@]}" -X DELETE "$BASE/_matrix/client/v3/room_keys/keys/$ROOM/$SESS?version=$VER")
echo "  $del"
pass "single-session delete returned"

echo "=== 15. DELETE /room_keys/version/$VER ==="
del=$("${CURL[@]}" "${AUTH[@]}" -X DELETE "$BASE/_matrix/client/v3/room_keys/version/$VER")
echo "  $del"
pass "version delete returned"

echo "=== 16. GET /room_keys/version after delete -> 404 ==="
code=$("${CURL[@]}" "${AUTH[@]}" -o /tmp/kb_resp.json -w "%{http_code}" "$BASE/_matrix/client/v3/room_keys/version")
[[ "$code" == "404" ]] || fail "post-delete latest" "404" "$code body=$(cat /tmp/kb_resp.json)"
pass "404 again after deletion"

echo
echo "All 16 checks passed."
