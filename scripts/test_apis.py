import requests
import json
import time

BASE_URL = "http://localhost:8008"
TOKEN = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW5fdGVzdGVyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkBhZG1pbl90ZXN0ZXI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMDA2NDQ2LCJpYXQiOjE3Njk5MjAwNDYsImRldmljZV9pZCI6Ii9BNXVSUVJqU3dBV2ZwbUY0L2dRZGc9PSJ9.cafEjcKR1WXTbBCC2I8ZbSULAHY6gkFks6WN0IaUGEQ"

headers = {
    "Authorization": f"Bearer {TOKEN}",
    "Content-Type": "application/json"
}

endpoints = [
    # General
    {"method": "GET", "path": "/_matrix/client/versions", "desc": "Get client versions", "auth": False},
    {"method": "GET", "path": "/_matrix/client/r0/account/whoami", "desc": "Who am I", "auth": True},
    
    # Registration & Login
    {"method": "GET", "path": "/_matrix/client/r0/register/available?username=testuser", "desc": "Check username availability", "auth": False},
    {"method": "POST", "path": "/_matrix/client/r0/login", "desc": "Login", "auth": False, "body": {"type": "m.login.password", "user": "admin_tester", "password": "password123"}},
    
    # Admin
    {"method": "GET", "path": "/_synapse/admin/v1/server_version", "desc": "Get server version", "auth": True},
    {"method": "GET", "path": "/_synapse/admin/v1/users", "desc": "List users", "auth": True},
    {"method": "GET", "path": "/_synapse/admin/v1/rooms", "desc": "List rooms", "auth": True},
    {"method": "GET", "path": "/_synapse/admin/v1/status", "desc": "Server status", "auth": True},
    
    # Profile
    {"method": "GET", "path": "/_matrix/client/r0/account/profile/@admin_tester:matrix.cjystx.top", "desc": "Get profile", "auth": True},
    
    # Sync
    {"method": "GET", "path": "/_matrix/client/r0/sync", "desc": "Initial sync", "auth": True},
    
    # Rooms
    {"method": "POST", "path": "/_matrix/client/r0/createRoom", "desc": "Create room", "auth": True, "body": {"name": "Test Room", "topic": "Test Topic", "visibility": "public"}},
    {"method": "GET", "path": "/_matrix/client/r0/publicRooms", "desc": "List public rooms", "auth": True},
    
    # Devices
    {"method": "GET", "path": "/_matrix/client/r0/devices", "desc": "List devices", "auth": True},
    
    # Friends (New Matrix-based API)
    {"method": "GET", "path": "/_matrix/client/v1/friends", "desc": "List friends", "auth": True},
    {"method": "POST", "path": "/_matrix/client/v1/friends/request", "desc": "Send friend request", "auth": True},

    # Voice (Enhanced)
    {"method": "GET", "path": "/_matrix/client/r0/voice/user/@admin_tester:matrix.cjystx.top/stats", "desc": "Voice stats", "auth": True},
]

results = []

for ep in endpoints:
    url = f"{BASE_URL}{ep['path']}"
    method = ep['method']
    auth = ep['auth']
    body = ep.get('body')
    
    req_headers = headers if auth else {}
    
    start_time = time.time()
    try:
        if method == "GET":
            resp = requests.get(url, headers=req_headers, timeout=10)
        elif method == "POST":
            resp = requests.post(url, headers=req_headers, json=body, timeout=10)
        elif method == "PUT":
            resp = requests.put(url, headers=req_headers, json=body, timeout=10)
        elif method == "DELETE":
            resp = requests.delete(url, headers=req_headers, timeout=10)
        else:
            continue
            
        duration = (time.time() - start_time) * 1000
        
        status = "OK" if resp.status_code < 400 else "ERROR"
        if resp.status_code == 404:
            status = "NOT_FOUND"
        elif resp.status_code == 501:
            status = "NOT_IMPLEMENTED"
            
        results.append({
            "method": method,
            "path": ep['path'],
            "desc": ep['desc'],
            "status_code": resp.status_code,
            "duration_ms": duration,
            "status": status,
            "response": resp.json() if resp.headers.get("content-type") == "application/json" else resp.text[:100]
        })
    except Exception as e:
        results.append({
            "method": method,
            "path": ep['path'],
            "desc": ep['desc'],
            "status_code": 0,
            "duration_ms": 0,
            "status": "EXCEPTION",
            "response": str(e)
        })

print(json.dumps(results, indent=2, ensure_ascii=False))
