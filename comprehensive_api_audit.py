import requests
import time
import json
import statistics

BASE_URL = "http://localhost:8008"

class MatrixClient:
    def __init__(self, username, password):
        self.username = username
        self.password = password
        self.token = None
        self.user_id = None
        self.headers = {}

    def login(self):
        resp = requests.post(f"{BASE_URL}/_matrix/client/r0/login", json={
            "type": "m.login.password",
            "user": self.username,
            "password": self.password
        })
        if resp.status_code == 200:
            data = resp.json()
            self.token = data["access_token"]
            self.user_id = data["user_id"]
            self.headers = {"Authorization": f"Bearer {self.token}"}
            return True
        return False

def test_api(name, method, path, client=None, payload=None, params=None, expected_status=200):
    start_time = time.time()
    headers = client.headers if client else {}
    url = f"{BASE_URL}{path}"
    
    try:
        if method == "GET":
            resp = requests.get(url, headers=headers, params=params, timeout=10)
        elif method == "POST":
            resp = requests.post(url, headers=headers, json=payload, timeout=10)
        elif method == "PUT":
            resp = requests.put(url, headers=headers, json=payload, timeout=10)
        elif method == "DELETE":
            resp = requests.delete(url, headers=headers, timeout=10)
        
        duration = (time.time() - start_time) * 1000
        status = "✅ PASS" if resp.status_code == expected_status else f"❌ FAIL ({resp.status_code})"
        
        return {
            "name": name,
            "path": path,
            "method": method,
            "status": status,
            "duration": f"{duration:.2f}ms",
            "code": resp.status_code,
            "response": resp.text[:200]
        }
    except Exception as e:
        return {
            "name": name,
            "path": path,
            "method": method,
            "status": f"💥 ERROR ({str(e)})",
            "duration": "N/A",
            "code": 0,
            "response": ""
        }

def run_audit():
    admin = MatrixClient("admin_test", "admin_pass")
    normal = MatrixClient("normal_test", "normal_pass")
    
    if not admin.login():
        print("Admin login failed")
        return
    if not normal.login():
        print("Normal user login failed")
        return

    results = []
    
    # 1. Core Client APIs
    print("Testing Core Client APIs...")
    results.append(test_api("Server Info", "GET", "/"))
    results.append(test_api("Health", "GET", "/health"))
    results.append(test_api("Versions", "GET", "/_matrix/client/versions"))
    results.append(test_api("WhoAmI (Admin)", "GET", "/_matrix/client/r0/account/whoami", client=admin))
    results.append(test_api("WhoAmI (Normal)", "GET", "/_matrix/client/r0/account/whoami", client=normal))
    results.append(test_api("Sync (Normal)", "GET", "/_matrix/client/r0/sync", client=normal))
    
    # 2. Admin APIs
    print("Testing Admin APIs...")
    results.append(test_api("Admin: Server Version", "GET", "/_synapse/admin/v1/server_version", client=admin))
    results.append(test_api("Admin: List Users", "GET", "/_synapse/admin/v1/users", client=admin))
    results.append(test_api("Admin: List Rooms", "GET", "/_synapse/admin/v1/rooms", client=admin))
    results.append(test_api("Admin: IP Blocks", "GET", "/_synapse/admin/v1/security/ip/blocks", client=admin))
    
    # Permission Test (Normal user accessing Admin API)
    results.append(test_api("Admin: List Users (Unauthorized)", "GET", "/_synapse/admin/v1/users", client=normal, expected_status=403))

    # 3. Federation APIs
    print("Testing Federation APIs...")
    results.append(test_api("Federation: Server Key", "GET", "/_matrix/federation/v2/server"))
    results.append(test_api("Federation: Version", "GET", "/_matrix/federation/v1/version"))

    # 4. Enhanced Features
    print("Testing Enhanced APIs...")
    results.append(test_api("Friends: List", "GET", "/_synapse/enhanced/friends", client=normal))
    results.append(test_api("Friends: Search", "GET", "/_synapse/enhanced/friends/search", client=normal, params={"search_term": "admin"}))
    results.append(test_api("DM: List", "GET", "/_matrix/client/r0/dm", client=normal))
    results.append(test_api("Voice: Stats", "GET", "/_matrix/client/r0/voice/stats", client=normal))

    # Boundary / Exception Testing
    print("Testing Boundary & Error Cases...")
    results.append(test_api("Sync (Invalid Token)", "GET", "/_matrix/client/r0/sync", client=MatrixClient("",""), expected_status=401))
    results.append(test_api("WhoAmI (Invalid Token)", "GET", "/_matrix/client/r0/account/whoami", client=MatrixClient("",""), expected_status=401))
    
    with open("audit_results.json", "w") as f:
        json.dump(results, f, indent=4)
    
    print(f"Audit completed. {len(results)} tests performed.")

if __name__ == "__main__":
    run_audit()
