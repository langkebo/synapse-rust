#!/usr/bin/env python3
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


BASE_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:28008"
OUTPUT_PATH = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("test-results/permission-matrix.json")

ACCOUNTS = {
    "super_admin": {"username": "sec_super_admin", "password": "Test@123"},
    "admin": {"username": "sec_admin", "password": "Test@123"},
    "user": {"username": "sec_user", "password": "Test@123"},
}


def http_json(method: str, path: str, token: str | None = None, body=None):
    url = urllib.parse.urljoin(BASE_URL, path)
    data = None
    headers = {}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as resp:
            raw = resp.read().decode()
            parsed = json.loads(raw) if raw else {}
            return resp.status, parsed, raw
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode()
        try:
            parsed = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            parsed = {"raw": raw}
        return exc.code, parsed, raw


def login(role: str):
    account = ACCOUNTS[role]
    status, parsed, _ = http_json(
        "POST",
        "/_matrix/client/v3/login",
        body={
            "type": "m.login.password",
            "user": account["username"],
            "password": account["password"],
        },
    )
    if status != 200:
        raise RuntimeError(f"login failed for {role}: {status} {parsed}")
    return parsed


def register_user(username: str, password: str):
    status, parsed, _ = http_json(
        "POST",
        "/_matrix/client/v3/register",
        body={
            "username": username,
            "password": password,
            "auth": {"type": "m.login.dummy"},
        },
    )
    if status != 200:
        raise RuntimeError(f"register failed for {username}: {status} {parsed}")
    return parsed


def status_matches(status: int, expected: str) -> bool:
    if expected == "allow":
        return 200 <= status < 300
    if expected == "deny":
        return status in (401, 403)
    if expected == "missing":
        return status == 404
    return False


def truncate(value: str, limit: int = 400) -> str:
    return value if len(value) <= limit else value[:limit] + "...(truncated)"


def run_case(results, unauthorized_successes, sessions, case, role):
    token = sessions[role]["access_token"]
    status, parsed, raw = http_json(
        case["method"],
        case["path"],
        token=token,
        body=case.get("body"),
    )
    expected = case["expected"][role]
    matched = status_matches(status, expected)
    entry = {
        "case": case["name"],
        "role": role,
        "expected": expected,
        "status": status,
        "matched": matched,
        "body": parsed,
        "body_excerpt": truncate(raw),
    }
    results.append(entry)
    if expected == "deny" and 200 <= status < 300:
        unauthorized_successes.append(entry)


def main():
    sessions = {role: login(role) for role in ACCOUNTS}
    user_id = sessions["user"]["user_id"]
    admin_user_id = sessions["admin"]["user_id"]
    encoded_user_id = urllib.parse.quote(user_id, safe="")
    encoded_admin_user_id = urllib.parse.quote(admin_user_id, safe="")
    probe_suffix = int(time.time())

    deactivate_probe = register_user(f"sec_deactivate_probe_{probe_suffix}", "Test@123")
    deactivate_probe_user_id = deactivate_probe["user_id"]
    encoded_deactivate_probe_user_id = urllib.parse.quote(deactivate_probe_user_id, safe="")

    login_probe = register_user(f"sec_login_probe_{probe_suffix}", "Test@123")
    login_probe_user_id = login_probe["user_id"]
    encoded_login_probe_user_id = urllib.parse.quote(login_probe_user_id, safe="")

    room_status, room_body, _ = http_json(
        "POST",
        "/_matrix/client/v3/createRoom",
        token=sessions["super_admin"]["access_token"],
        body={
            "name": f"Permission Probe Room {probe_suffix}",
            "visibility": "private",
        },
    )
    if room_status != 200:
        raise RuntimeError(f"failed to create reference room: {room_status} {room_body}")
    room_id = room_body["room_id"]
    encoded_room_id = urllib.parse.quote(room_id, safe="")

    blacklist_server_name = f"probe-blocked-{probe_suffix}.example.com"

    filter_status, filter_body, _ = http_json(
        "POST",
        f"/_matrix/client/v3/user/{encoded_user_id}/filter",
        token=sessions["user"]["access_token"],
        body={"room": {"timeline": {"limit": 10}}},
    )
    if filter_status != 200:
        raise RuntimeError(f"failed to create reference filter: {filter_status} {filter_body}")
    filter_id = filter_body["filter_id"]

    cases = [
        {
            "name": "Admin List Users",
            "method": "GET",
            "path": "/_synapse/admin/v1/users",
            "expected": {"super_admin": "allow", "admin": "allow", "user": "deny"},
        },
        {
            "name": "Admin Get User v2",
            "method": "GET",
            "path": f"/_synapse/admin/v2/users/{encoded_user_id}",
            "expected": {"super_admin": "allow", "admin": "allow", "user": "deny"},
        },
        {
            "name": "Admin Federation Resolve",
            "method": "POST",
            "path": "/_synapse/admin/v1/federation/resolve",
            "body": {"server_name": "localhost"},
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Admin User Login",
            "method": "POST",
            "path": f"/_synapse/admin/v1/users/{encoded_login_probe_user_id}/login",
            "body": {"password": "Test@123"},
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Admin User Logout",
            "method": "POST",
            "path": f"/_synapse/admin/v1/users/{encoded_login_probe_user_id}/logout",
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Admin User Deactivate",
            "method": "POST",
            "path": f"/_synapse/admin/v1/users/{encoded_deactivate_probe_user_id}/deactivate",
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Admin Room Make Admin",
            "method": "PUT",
            "path": f"/_synapse/admin/v1/rooms/{encoded_room_id}/make_admin",
            "body": {"user_id": admin_user_id},
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Admin Federation Blacklist",
            "method": "POST",
            "path": f"/_synapse/admin/v1/federation/blacklist/{blacklist_server_name}",
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Admin Federation Cache Clear",
            "method": "POST",
            "path": "/_synapse/admin/v1/federation/cache/clear",
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
        {
            "name": "Foreign Filter Create",
            "method": "POST",
            "path": f"/_matrix/client/v3/user/{encoded_user_id}/filter",
            "body": {"room": {"timeline": {"limit": 1}}},
            "subject_roles": ["super_admin", "admin"],
            "expected": {"super_admin": "deny", "admin": "deny"},
        },
        {
            "name": "Foreign Filter Read",
            "method": "GET",
            "path": f"/_matrix/client/v3/user/{encoded_user_id}/filter/{filter_id}",
            "subject_roles": ["super_admin", "admin"],
            "expected": {"super_admin": "deny", "admin": "deny"},
        },
        {
            "name": "Foreign OpenID Request",
            "method": "GET",
            "path": f"/_matrix/client/v3/user/{encoded_user_id}/openid/request_token",
            "subject_roles": ["super_admin", "admin"],
            "expected": {"super_admin": "deny", "admin": "deny"},
        },
        {
            "name": "Foreign Account Data Write",
            "method": "PUT",
            "path": f"/_matrix/client/v3/user/{encoded_user_id}/account_data/m.probe",
            "body": {"probe": True},
            "subject_roles": ["super_admin", "admin"],
            "expected": {"super_admin": "deny", "admin": "deny"},
        },
        {
            "name": "Foreign Presence Update",
            "method": "PUT",
            "path": f"/_matrix/client/v3/presence/{encoded_user_id}/status",
            "body": {"presence": "online", "status_msg": "probe"},
            "subject_roles": ["super_admin", "admin"],
            "expected": {"super_admin": "deny", "admin": "deny"},
        },
        {
            "name": "Admin Shutdown Room",
            "method": "POST",
            "path": "/_synapse/admin/v1/shutdown_room",
            "body": {"room_id": room_id},
            "expected": {"super_admin": "allow", "admin": "deny", "user": "deny"},
        },
    ]

    results = []
    unauthorized_successes = []

    for case in cases:
        subject_roles = case.get("subject_roles", list(ACCOUNTS))
        for role in subject_roles:
            run_case(results, unauthorized_successes, sessions, case, role)

    worker_id = f"probe-worker-{int(time.time())}"
    for role in ("super_admin", "admin", "user"):
        register_case = {
            "name": f"Worker Register ({role})",
            "method": "POST",
            "path": "/_synapse/worker/v1/register",
            "body": {
                "worker_id": f"{worker_id}-{role}",
                "worker_name": f"Permission Probe Worker {role}",
                "worker_type": "frontend",
                "host": "127.0.0.1",
                "port": 9100,
            },
            "expected": {"super_admin": "allow", "admin": "allow", "user": "deny"},
        }
        run_case(results, unauthorized_successes, sessions, register_case, role)

    worker_setup_status, worker_setup_body, _ = http_json(
        "POST",
        "/_synapse/worker/v1/register",
        token=sessions["super_admin"]["access_token"],
        body={
            "worker_id": worker_id,
            "worker_name": "Permission Probe Claim Worker",
            "worker_type": "frontend",
            "host": "127.0.0.1",
            "port": 9101,
        },
    )
    if worker_setup_status not in (200, 201):
        raise RuntimeError(
            f"failed to register worker for claim probe: {worker_setup_status} {worker_setup_body}"
        )

    for role in ("super_admin", "admin", "user"):
        assign_status, assign_body, _ = http_json(
            "POST",
            "/_synapse/worker/v1/tasks",
            token=sessions["super_admin"]["access_token"],
            body={
                "task_type": "sync",
                "task_data": {"probe_role": role},
                "preferred_worker_id": None,
            },
        )
        if assign_status != 201:
            raise RuntimeError(
                f"failed to create worker task for {role}: {assign_status} {assign_body}"
            )

        claim_case = {
            "name": f"Worker Task Claim ({role})",
            "method": "POST",
            "path": f"/_synapse/worker/v1/tasks/claim/{worker_id}",
            "expected": {
                "super_admin": "allow" if role == "super_admin" else "allow",
                "admin": "allow" if role == "admin" else "allow",
                "user": "deny",
            },
        }
        run_case(results, unauthorized_successes, sessions, claim_case, role)

    report = {
        "base_url": BASE_URL,
        "accounts": {role: {"user_id": sessions[role]["user_id"]} for role in sessions},
        "reference_filter_id": filter_id,
        "results": results,
        "unauthorized_successes": unauthorized_successes,
        "summary": {
            "total": len(results),
            "matched": sum(1 for item in results if item["matched"]),
            "mismatched": sum(1 for item in results if not item["matched"]),
            "unauthorized_success_count": len(unauthorized_successes),
        },
    }

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report["summary"], ensure_ascii=False))


if __name__ == "__main__":
    main()
