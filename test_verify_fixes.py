#!/usr/bin/env python3
import urllib.request
import urllib.parse
import json
import ssl
import time
import base64
import os

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE
BASE = "https://matrix.test"


def api(
    method,
    path,
    token=None,
    data=None,
    content_type="application/json",
    raw_body=None,
    headers_extra=None,
):
    url = f"{BASE}{path}"
    headers = {}
    if content_type:
        headers["Content-Type"] = content_type
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if headers_extra:
        headers.update(headers_extra)
    if raw_body is not None:
        body = raw_body
    elif data is not None:
        body = json.dumps(data).encode()
    else:
        body = None
    req = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=30) as resp:
            try:
                return json.loads(resp.read().decode()), resp.status, dict(resp.headers)
            except:
                return {}, resp.status, dict(resp.headers)
    except urllib.error.HTTPError as e:
        try:
            err_body = e.read().decode()
            return json.loads(err_body), e.code, {}
        except:
            return {"raw_error": str(e)}, e.code, {}
    except Exception as e:
        return {"connection_error": str(e)}, 0, {}


pass_count = 0
fail_count = 0
results = []


def check(name, method, path, token=None, data=None, expected_codes=None):
    global pass_count, fail_count
    resp, code, hdrs = api(method, path, token, data)
    ok_codes = expected_codes or [200, 201]
    has_error = "errcode" in resp
    if code in ok_codes and not has_error:
        pass_count += 1
        results.append(f"✅ {name}: {code}")
        return True, resp, code, hdrs
    else:
        fail_count += 1
        err = resp.get("errcode", "") + " " + resp.get("error", "")[:60]
        results.append(f"❌ {name}: {code} {err}")
        return False, resp, code, hdrs


# Register and login
reg, code, _ = api(
    "POST",
    "/_matrix/client/v3/register",
    data={
        "username": f"verify5_{int(time.time())}",
        "password": "Test1234!Abc",
        "auth": {"type": "m.login.dummy"},
    },
)
TOKEN1 = reg.get("access_token", "")
USER1 = reg.get("user_id", "")
DEVICE1 = reg.get("device_id", "")
print(f"User: {USER1}")

reg2, code2, _ = api(
    "POST",
    "/_matrix/client/v3/register",
    data={
        "username": f"verify5b_{int(time.time())}",
        "password": "Test1234!Abc",
        "auth": {"type": "m.login.dummy"},
    },
)
TOKEN2 = reg2.get("access_token", "")
USER2 = reg2.get("user_id", "")

# ============================================================
# P0-1: CORS Allow-Origin
# ============================================================
print("\n=== P0-1: CORS ===")
req = urllib.request.Request(f"{BASE}/_matrix/client/versions", method="OPTIONS")
req.add_header("Origin", "https://element.test")
req.add_header("Access-Control-Request-Method", "GET")
try:
    with urllib.request.urlopen(req, context=ctx) as resp:
        origin = resp.headers.get("Access-Control-Allow-Origin", "NOT SET")
        creds = resp.headers.get("Access-Control-Allow-Credentials", "NOT SET")
        if origin == "https://element.test":
            pass_count += 1
            results.append(f"✅ CORS Allow-Origin: {origin}")
            print(f"  ✅ CORS: origin={origin}, credentials={creds}")
        else:
            fail_count += 1
            results.append(f"❌ CORS Allow-Origin: {origin}")
            print(f"  ❌ CORS: origin={origin}")
except urllib.error.HTTPError as e:
    fail_count += 1
    results.append(f"❌ CORS: {e.code}")
    print(f"  ❌ CORS: {e.code}")

# ============================================================
# P0-2: State Event Format
# ============================================================
print("\n=== P0-2: State Event Format ===")
room, _, _ = api(
    "POST", "/_matrix/client/v3/createRoom", TOKEN1, {"name": "Verify Format"}
)
ROOM_ID = room.get("room_id", "")

# Without trailing slash (Element Web's actual request)
resp, code, _ = api(
    "GET", f"/_matrix/client/v3/rooms/{ROOM_ID}/state/m.room.name", TOKEN1
)
if code == 200 and isinstance(resp, dict):
    if "name" in resp and "events" not in resp:
        pass_count += 1
        results.append("✅ State Event Format (no slash): content object")
        print(f"  ✅ /state/m.room.name: {resp}")
    elif "events" in resp:
        fail_count += 1
        results.append("❌ State Event Format (no slash): events wrapper")
        print(f"  ❌ /state/m.room.name: returns events wrapper")
    else:
        fail_count += 1
        results.append(f"❌ State Event Format: unexpected {list(resp.keys())}")
        print(f"  ❌ /state/m.room.name: unexpected {list(resp.keys())}")

# With trailing slash
resp2, code2, _ = api(
    "GET", f"/_matrix/client/v3/rooms/{ROOM_ID}/state/m.room.name/", TOKEN1
)
if code2 == 200 and isinstance(resp2, dict) and "name" in resp2:
    pass_count += 1
    results.append("✅ State Event Format (with slash): content object")
    print(f"  ✅ /state/m.room.name/: {resp2}")
else:
    fail_count += 1
    results.append(f"❌ State Event Format (with slash): {code2}")
    print(f"  ❌ /state/m.room.name/: {code2} {resp2}")

# ============================================================
# P0-3: Keys Upload (empty device_keys)
# ============================================================
print("\n=== P0-3: Keys Upload ===")
resp, code, _ = api(
    "POST", "/_matrix/client/v3/keys/upload", TOKEN1, {"one_time_keys": {}}
)
if code == 200:
    pass_count += 1
    results.append("✅ Keys Upload (empty): 200")
    print(f"  ✅ Empty keys upload: {code} {resp}")
else:
    fail_count += 1
    results.append(f"❌ Keys Upload (empty): {code}")
    print(f"  ❌ Empty keys upload: {code} {resp}")

# ============================================================
# P0-4: Wrong Password Error Code
# ============================================================
print("\n=== P0-4: Wrong Password ===")
resp, code, _ = api(
    "POST",
    "/_matrix/client/v3/login",
    data={
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": USER1.split(":")[0][1:]},
        "password": "WrongPassword",
    },
)
if code == 403 and resp.get("errcode") == "M_FORBIDDEN":
    pass_count += 1
    results.append("✅ Wrong Password: 403 M_FORBIDDEN")
    print(f"  ✅ Wrong password: {code} {resp.get('errcode')}")
else:
    fail_count += 1
    results.append(f"❌ Wrong Password: {code} {resp.get('errcode', '')}")
    print(f"  ❌ Wrong password: {code} {resp}")

# ============================================================
# P1-5: Sync Ephemeral (Typing + Receipt)
# ============================================================
print("\n=== P1-5: Sync Ephemeral ===")
# Send typing
api(
    "PUT",
    f"/_matrix/client/v3/rooms/{ROOM_ID}/typing/{USER1}",
    TOKEN1,
    {"typing": True, "timeout": 30000},
)
time.sleep(1)

# Send message and receipt
msg, _, _ = api(
    "PUT",
    f"/_matrix/client/v3/rooms/{ROOM_ID}/send/m.room.message/txn_verify",
    TOKEN1,
    {"msgtype": "m.text", "body": "verify test"},
)
EVENT_ID = msg.get("event_id", "")
if EVENT_ID:
    api(
        "POST",
        f"/_matrix/client/v3/rooms/{ROOM_ID}/receipt/m.read/{EVENT_ID}",
        TOKEN1,
        {},
    )
    time.sleep(1)

# Sync
sync, code, _ = api("GET", "/_matrix/client/v3/sync?timeout=0", TOKEN1)
found_typing = False
found_receipt = False
for rid, room_data in sync.get("rooms", {}).get("join", {}).items():
    ephemeral = room_data.get("ephemeral", {}).get("events", [])
    for evt in ephemeral:
        if evt.get("type") == "m.typing":
            found_typing = True
        if evt.get("type") == "m.receipt":
            found_receipt = True

if found_typing:
    pass_count += 1
    results.append("✅ Sync Typing Ephemeral")
    print(f"  ✅ Typing in sync")
else:
    fail_count += 1
    results.append("❌ Sync Typing Ephemeral: not found")
    print(f"  ❌ Typing not in sync")

if found_receipt:
    pass_count += 1
    results.append("✅ Sync Receipt Ephemeral")
    print(f"  ✅ Receipt in sync")
else:
    fail_count += 1
    results.append("❌ Sync Receipt Ephemeral: not found")
    print(f"  ❌ Receipt not in sync")

# ============================================================
# P1-6: Media Content-Type
# ============================================================
print("\n=== P1-6: Media Content-Type ===")
try:
    upload_req = urllib.request.Request(
        f"{BASE}/_matrix/media/v3/upload?filename=test_ct.txt",
        data=b"test content for content type",
        headers={"Authorization": f"Bearer {TOKEN1}", "Content-Type": "text/plain"},
        method="POST",
    )
    with urllib.request.urlopen(upload_req, context=ctx) as resp:
        upload = json.loads(resp.read().decode())
        mxc = upload.get("content_uri", "")
        if mxc:
            parts = mxc.split("/")
            server = parts[2]
            mid = parts[3]
            dl_req = urllib.request.Request(
                f"{BASE}/_matrix/media/v3/download/{server}/{mid}"
            )
            with urllib.request.urlopen(dl_req, context=ctx) as dl_resp:
                ct = dl_resp.headers.get("Content-Type", "")
                if "text/plain" in ct:
                    pass_count += 1
                    results.append(f"✅ Media Content-Type: {ct}")
                    print(f"  ✅ Content-Type: {ct}")
                else:
                    fail_count += 1
                    results.append(f"❌ Media Content-Type: {ct}")
                    print(f"  ❌ Content-Type: {ct} (expected text/plain)")
except urllib.error.HTTPError as e:
    fail_count += 1
    results.append(f"❌ Media Content-Type: upload/download error {e.code}")
    print(f"  ❌ Error: {e.code}")

# ============================================================
# P1-7: Presence List GET
# ============================================================
print("\n=== P1-7: Presence List GET ===")
resp, code, _ = api("GET", "/_matrix/client/v3/presence/list", TOKEN1)
if code == 200:
    pass_count += 1
    results.append("✅ Presence List GET: 200")
    print(f"  ✅ Presence List GET: {code}")
else:
    fail_count += 1
    results.append(f"❌ Presence List GET: {code}")
    print(f"  ❌ Presence List GET: {code} {resp}")

# ============================================================
# P1-8: Guest Registration
# ============================================================
print("\n=== P1-8: Guest Registration ===")
resp, code, _ = api("POST", "/_matrix/client/v3/register", data={"kind": "guest"})
if code in [200, 201] and "access_token" in resp:
    pass_count += 1
    results.append("✅ Guest Registration: OK")
    print(f"  ✅ Guest Registration: {resp.get('user_id', '')}")
else:
    fail_count += 1
    results.append(f"❌ Guest Registration: {code}")
    print(f"  ❌ Guest Registration: {code} {resp}")

# ============================================================
# Additional verification tests
# ============================================================
print("\n=== Additional Verification ===")

# Redact
if EVENT_ID:
    check(
        "Redact",
        "PUT",
        f"/_matrix/client/v3/rooms/{ROOM_ID}/redact/{EVENT_ID}/txn_redact_v",
        TOKEN1,
        {"reason": "verify"},
    )

# Receipt
if EVENT_ID:
    check(
        "Receipt",
        "POST",
        f"/_matrix/client/v3/rooms/{ROOM_ID}/receipt/m.read/{EVENT_ID}",
        TOKEN1,
        {},
    )

# Read Markers
if EVENT_ID:
    check(
        "Read Markers",
        "POST",
        f"/_matrix/client/v3/rooms/{ROOM_ID}/read_markers",
        TOKEN1,
        {"m.fully_read": EVENT_ID, "m.read": EVENT_ID},
    )

# Media upload/download
check(
    "Media Upload",
    "POST",
    "/_matrix/media/v3/upload?filename=v.txt",
    TOKEN1,
    raw_body=b"verify",
    content_type="text/plain",
    headers_extra={"Authorization": f"Bearer {TOKEN1}"},
)

# Auth media download
check("Auth Media Config", "GET", "/_matrix/client/v1/media/config", TOKEN1)

# Capabilities
check("Capabilities", "GET", "/_matrix/client/v3/capabilities", TOKEN1)

# Profile
check("Profile", "GET", f"/_matrix/client/v3/profile/{USER1}", TOKEN1)

# ============================================================
# Summary
# ============================================================
print("\n" + "=" * 60)
print("验证测试结果")
print("=" * 60)
for r in results:
    print(f"  {r}")
print(f"\n通过: {pass_count}  失败: {fail_count}  总计: {pass_count + fail_count}")
print(f"通过率: {pass_count / (pass_count + fail_count) * 100:.1f}%")
