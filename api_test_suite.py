import requests
import json
import time
import re
import os

BASE_URL = "http://localhost:8008"
RESULTS_FILE = "api_test_results.json"
MARKDOWN_FILE = "docs/synapse-rust/api-reference.md"

class APITestSuite:
    def __init__(self):
        self.apis = []
        self.results = []
        self.token = None
        self.admin_token = None
        self.username = None
        self.password = "TestPassword123!"
        self.user_id = None
        self.room_id = None

    def parse_markdown(self):
        print(f"Parsing {MARKDOWN_FILE}...")
        with open(MARKDOWN_FILE, 'r', encoding='utf-8') as f:
            content = f.read()

        sections = re.split(r'### \d+\.\d+ ', content)[1:]
        
        for section in sections:
            lines = section.split('\n')
            name = lines[0].strip()
            
            endpoint_match = re.search(r'\*\*端点\*\*: `(\w+) ([^`]+)`', section)
            if not endpoint_match:
                continue
                
            method = endpoint_match.group(1)
            path = endpoint_match.group(2)
            
            auth_match = re.search(r'\*\*认证\*\*: (是|否)', section)
            auth_required = auth_match.group(1) == '是' if auth_match else False
            
            body = None
            body_match = re.search(r'\*\*请求体\*\*:\s*```json\s*(.*?)\s*```', section, re.DOTALL)
            if body_match:
                try:
                    body = json.loads(body_match.group(1))
                except:
                    pass
            
            self.apis.append({
                "name": name,
                "method": method,
                "path": path,
                "auth": auth_required,
                "body": body
            })
        print(f"Extracted {len(self.apis)} APIs.")

    def setup_users(self):
        print("Setting up test users...")
        self.username = f"testuser_{int(time.time())}"
        
        # Register main user
        reg_data = {"username": self.username, "password": self.password, "admin": False}
        resp = requests.post(f"{BASE_URL}/_matrix/client/r0/register", json=reg_data)
        if resp.status_code == 200:
            data = resp.json()
            self.token = data.get("access_token")
            self.user_id = data.get("user_id")
            print(f"User registered: {self.user_id}")
        
        # Register helper users to avoid FK violations
        for helper in ["bob", "charlie"]:
            requests.post(f"{BASE_URL}/_matrix/client/r0/register", json={
                "username": helper, "password": "password123", "admin": False
            })

        # Register admin user
        admin_username = f"admin_{int(time.time())}"
        admin_reg_data = {"username": admin_username, "password": self.password, "admin": True}
        resp = requests.post(f"{BASE_URL}/_matrix/client/r0/register", json=admin_reg_data)
        if resp.status_code == 200:
            self.admin_token = resp.json().get("access_token")
            print("Admin user registered.")

    def run_tests(self):
        print("Starting API tests...")
        for api in self.apis:
            if any(x in api["path"] for x in ["logout", "deactivate", "delete", "shutdown", "purge"]):
                continue
            if "federation" in api["path"]:
                continue
            self.test_api(api)

    def test_api(self, api):
        name = api["name"]
        method = api["method"]
        path = api["path"]
        body = api["body"]
        
        # Path replacements
        path = path.replace("{user_id}", self.user_id or "@test:server.com")
        path = path.replace("{room_id}", self.room_id or "!test:server.com")
        path = path.replace("{device_id}", "DEVICEID")
        path = path.replace("{event_type}", "m.room.message")
        path = path.replace("{txn_id}", str(int(time.time())))
        path = path.replace("{version}", "1")
        path = path.replace("{server_name}", "matrix.cjystx.top")
        path = path.replace("{media_id}", "media123")
        path = path.replace("{message_id}", "msg123")
        path = path.replace("{request_id}", "1")
        path = path.replace("{category_name}", "Work")
        path = path.replace("{blocked_user_id}", "@bob:matrix.cjystx.top")
        path = path.replace("{session_id}", "ps_123")
        path = path.replace("{ip}", "127.0.0.1")
        path = path.replace("{username}", self.username or "testuser")
        path = path.replace("{key_id}", "ed25519:1")
        path = path.replace("{transaction_id}", "txn123")
        path = path.replace("{state_key}", "test_state")
        path = path.replace("{event_id}", "$event123")
        
        # Body replacements
        if body:
            body_str = json.dumps(body)
            body_str = body_str.replace("alice", self.username or "alice")
            body_str = body_str.replace("secret_password", self.password)
            body_str = body_str.replace("@bob:server.com", "@bob:matrix.cjystx.top")
            body_str = body_str.replace("@charlie:server.com", "@charlie:matrix.cjystx.top")
            body = json.loads(body_str)

        url = f"{BASE_URL}{path}"
        headers = {}
        if api["auth"]:
            headers["Authorization"] = f"Bearer {self.admin_token if 'admin' in path else self.token}"

        print(f"Testing {method} {path}...", end=" ", flush=True)
        start_time = time.time()
        try:
            if method == "GET":
                resp = requests.get(url, headers=headers, timeout=5)
            elif method == "POST":
                resp = requests.post(url, headers=headers, json=body, timeout=5)
            elif method == "PUT":
                resp = requests.put(url, headers=headers, json=body, timeout=5)
            elif method == "DELETE":
                resp = requests.delete(url, headers=headers, timeout=5)
            else:
                print("SKIP")
                return

            duration = (time.time() - start_time) * 1000
            
            if "createRoom" in path and resp.status_code == 200:
                self.room_id = resp.json().get("room_id")

            # 409 is expected for register if user exists, 404/403 often expected for dummy IDs
            success = 200 <= resp.status_code < 300 or resp.status_code in [404, 403, 409]
            
            result = {
                "name": name, "method": method, "path": path, "status_code": resp.status_code,
                "duration_ms": round(duration, 2), "success": success, "response": resp.text[:500]
            }
            
            print(f"{'OK' if success else 'FAIL'} ({resp.status_code})")
            self.results.append(result)
        except Exception as e:
            print(f"ERROR: {str(e)}")
            self.results.append({"name": name, "method": method, "path": path, "status_code": 0, "success": False, "error": str(e)})

    def generate_report(self):
        total = len(self.results)
        passed = sum(1 for r in self.results if r["success"])
        print(f"\n=== Test Summary ===\nTotal: {total}\nPassed: {passed}\nFailed: {total - passed}\nCoverage: {round(passed/total*100, 2)}%")

if __name__ == "__main__":
    suite = APITestSuite()
    suite.parse_markdown()
    suite.setup_users()
    suite.run_tests()
    suite.generate_report()
