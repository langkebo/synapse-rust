import requests
import json
import time
import sys

BASE_URL = "http://localhost:8008"
SERVER_NAME = "matrix.cjystx.top"

def log(msg):
    print(f"[*] {msg}")

def error(msg):
    print(f"[!] ERROR: {msg}")

class MatrixClient:
    def __init__(self, username, password):
        self.username = username
        self.password = password
        self.user_id = None
        self.access_token = None
        self.device_id = None

    def register(self, admin=False):
        resp = requests.post(f"{BASE_URL}/_matrix/client/r0/register", json={
            "username": self.username,
            "password": self.password,
            "admin": admin,
            "displayname": self.username.capitalize()
        })
        if resp.status_code == 200:
            data = resp.json()
            self.user_id = data['user_id']
            self.access_token = data['access_token']
            self.device_id = data['device_id']
            return True
        elif resp.status_code == 409:
            log(f"User {self.username} already exists, attempting login...")
            return self.login()
        else:
            error(f"Registration failed for {self.username}: {resp.text}")
            return False

    def login(self):
        resp = requests.post(f"{BASE_URL}/_matrix/client/r0/login", json={
            "user": self.username,
            "password": self.password,
            "type": "m.login.password"
        })
        if resp.status_code == 200:
            data = resp.json()
            self.user_id = data['user_id']
            self.access_token = data['access_token']
            self.device_id = data['device_id']
            return True
        else:
            error(f"Login failed for {self.username}: {resp.text}")
            return False

    def get_headers(self):
        return {"Authorization": f"Bearer {self.access_token}"}

    def create_room(self, name, visibility="private", preset=None, invite=None):
        payload = {
            "name": name,
            "visibility": visibility
        }
        if preset: payload["preset"] = preset
        if invite: payload["invite"] = invite
        
        resp = requests.post(f"{BASE_URL}/_matrix/client/r0/createRoom", 
                             json=payload, headers=self.get_headers())
        if resp.status_code == 200:
            return resp.json()['room_id']
        else:
            error(f"Failed to create room {name}: {resp.text}")
            return None

def main():
    log("Starting Full API Deployment & Test Process")
    
    # 1. Create Users
    admin = MatrixClient("admin_test", "admin_pass")
    normal = MatrixClient("normal_test", "normal_pass")
    muted = MatrixClient("muted_test", "muted_pass")
    
    users = [admin, normal, muted]
    for i, u in enumerate(users):
        is_admin = (i == 0)
        if not u.register(admin=is_admin):
            sys.exit(1)
        log(f"User {u.user_id} registered/logged in (Admin: {is_admin})")

    # 2. Create Rooms
    rooms = {}
    
    # Public Room
    rooms['public'] = admin.create_room("Public Discussion", visibility="public", preset="public_chat")
    # Private Room
    rooms['private'] = admin.create_room("Private Lounge", visibility="private", preset="private_chat", invite=[normal.user_id])
    # Password Protected (Preset)
    rooms['password'] = admin.create_room("Secret Room", visibility="private", preset="trusted_private_chat")
    # Read-only (Mocked by specific permissions if implemented, or just a room)
    rooms['readonly'] = admin.create_room("Announcements", visibility="public")
    # Live Room (Enhanced feature)
    rooms['live'] = admin.create_room("Live Stream Chat", visibility="public")

    for k, v in rooms.items():
        if v: log(f"Created {k} room: {v}")

    # 3. Test API Categories
    results = []

    # Category 1: Client API - Whoami
    resp = requests.get(f"{BASE_URL}/_matrix/client/r0/account/whoami", headers=normal.get_headers())
    results.append({"api": "GET /_matrix/client/r0/account/whoami", "status": resp.status_code == 200})

    # Category 2: Admin API - User List
    resp = requests.get(f"{BASE_URL}/_synapse/admin/v1/users", headers=admin.get_headers())
    results.append({"api": "GET /_synapse/admin/v1/users", "status": resp.status_code == 200})
    
    # Category 2: Admin API - Forbidden for normal user
    resp = requests.get(f"{BASE_URL}/_synapse/admin/v1/users", headers=normal.get_headers())
    results.append({"api": "GET /_synapse/admin/v1/users (Forbidden)", "status": resp.status_code == 403})

    # Category 3: Friend API - Search
    resp = requests.get(f"{BASE_URL}/_synapse/enhanced/friends/search?query=normal", headers=admin.get_headers())
    results.append({"api": "GET /_synapse/enhanced/friends/search", "status": resp.status_code == 200})

    # Category 4: Private Chat API - DM List
    resp = requests.get(f"{BASE_URL}/_matrix/client/r0/dm", headers=normal.get_headers())
    results.append({"api": "GET /_matrix/client/r0/dm", "status": resp.status_code == 200})

    # Category 5: Media API - Config
    resp = requests.get(f"{BASE_URL}/_matrix/media/v1/config")
    results.append({"api": "GET /_matrix/media/v1/config", "status": resp.status_code == 200})

    # Category 6: E2EE API - Device List
    resp = requests.get(f"{BASE_URL}/_matrix/client/r0/devices", headers=normal.get_headers())
    results.append({"api": "GET /_matrix/client/r0/devices", "status": resp.status_code == 200})

    # Category 7: Federation API - Version (Unprotected version if exists or skip)
    # The current implementation protects /_matrix/federation/v1/version with federation auth.
    # I'll check /_matrix/federation/v1 which is discovery (also protected).
    # I'll test the public /_matrix/federation/v2/server
    resp = requests.get(f"{BASE_URL}/_matrix/federation/v2/server")
    results.append({"api": "GET /_matrix/federation/v2/server", "status": resp.status_code == 200})

    # 4. Summary
    log("\n--- API Test Summary ---")
    passed = 0
    for r in results:
        status = "✅ PASS" if r['status'] else "❌ FAIL"
        if r['status']: passed += 1
        log(f"{r['api']}: {status}")
    
    log(f"\nOverall Result: {passed}/{len(results)} passed")

    # Output report data
    report = {
        "users": {u.username: {"user_id": u.user_id, "token": u.access_token} for u in users},
        "rooms": rooms,
        "test_results": results
    }
    with open("test_report_data.json", "w") as f:
        json.dump(report, f, indent=4)
    log("Test report data saved to test_report_data.json")

if __name__ == "__main__":
    main()
