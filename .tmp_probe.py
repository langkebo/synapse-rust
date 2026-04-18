import json
import urllib.error
import urllib.parse
import urllib.request

BASE = "http://localhost:28008"


def request(method, path, token=None, body=None, extra_headers=None):
    url = BASE + path
    headers = {}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if extra_headers:
        headers.update(extra_headers)
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"

    req = urllib.request.Request(url, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req) as resp:
            return resp.status, resp.read().decode()
    except urllib.error.HTTPError as err:
        return err.code, err.read().decode()


status, payload = request(
    "POST",
    "/_matrix/client/v3/login",
    body={
        "type": "m.login.password",
        "user": "testuser1",
        "password": "Test@123",
    },
)
print("LOGIN_STATUS", status)
print("LOGIN_BODY", payload)
login = json.loads(payload)
token = login.get("access_token", "")
user_id = login.get("user_id", "")
user_id_enc = urllib.parse.quote(user_id, safe="")

status, payload = request(
    "GET",
    f"/_synapse/admin/v1/users/{user_id_enc}",
    token=token,
)
print("ADMIN_USER_STATUS", status)
print("ADMIN_USER_BODY", payload)

status, payload = request(
    "DELETE",
    f"/_synapse/admin/v1/users/{user_id_enc}/shadow_ban",
    token=token,
)
print("UNSHADOW_STATUS", status)
print("UNSHADOW_BODY", payload)

status, payload = request(
    "POST",
    "/_matrix/client/v3/createRoom",
    token=token,
    body={"name": "debug room", "preset": "private_chat"},
)
print("CREATE_ROOM_STATUS", status)
print("CREATE_ROOM_BODY", payload)
room_id = json.loads(payload).get("room_id", "") if payload.startswith("{") else ""

room_id_enc = urllib.parse.quote(room_id, safe="")

status, payload = request(
    "GET",
    f"/_matrix/client/v3/rooms/{room_id_enc}/version",
    token=token,
)
print("ROOM_VERSION_STATUS", status)
print("ROOM_VERSION_BODY", payload)

status, payload = request(
    "POST",
    f"/_matrix/client/v3/user/{user_id_enc}/filter",
    token=token,
    body={"room": {"rooms": [room_id]}},
)
print("CREATE_FILTER_STATUS", status)
print("CREATE_FILTER_BODY", payload)
filter_body = json.loads(payload) if payload.startswith("{") else {}
filter_id = filter_body.get("filter_id", "")

status, payload = request(
    "GET",
    f"/_matrix/client/v3/user/{user_id_enc}/filter/{filter_id}",
    token=token,
)
print("GET_FILTER_STATUS", status)
print("GET_FILTER_BODY", payload)

status, payload = request(
    "POST",
    "/_matrix/client/v1/rendezvous",
    token=token,
    body={"intent": "login.reciprocate", "transport": "http.v1"},
)
print("RENDEZVOUS_CREATE_STATUS", status)
print("RENDEZVOUS_CREATE_BODY", payload)
rendezvous_body = json.loads(payload) if payload.startswith("{") else {}
session_id = rendezvous_body.get("session_id", "")
session_key = rendezvous_body.get("key", "")

status, payload = request(
    "GET",
    f"/_matrix/client/v1/rendezvous/{session_id}",
    token=token,
    extra_headers={"X-Matrix-Rendezvous-Key": session_key} if session_key else None,
)
print("RENDEZVOUS_GET_STATUS", status)
print("RENDEZVOUS_GET_BODY", payload)
