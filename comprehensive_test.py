import requests
import json
import time
import re

BASE_URL = "http://localhost:8008"

class MatrixTester:
    def __init__(self):
        self.tokens = {}
        self.user_ids = {}
        self.results = []
        self.data = {
            "room_id": "tester_room_id",
            "event_id": "tester_event_id",
            "device_id": "tester_device_id",
            "request_id": "tester_request_id",
            "session_id": "tester_session_id",
            "media_id": "tester_media_id",
            "message_id": "tester_message_id",
            "user_id": "@tester_user1:matrix.cjystx.top",
            "server_name": "matrix.cjystx.top",
            "version": "v1",
            "event_type": "m.room.message",
            "txn_id": "txn_123",
            "category_name": "friends",
            "blocked_user_id": "@tester_user2:matrix.cjystx.top",
            "ip": "127.0.0.1",
            "key_id": "ed25519:123",
            "transaction_id": "t_123"
        }

    def setup_accounts(self):
        print("Setting up accounts...")
        for u in ["tester_admin", "tester_user1", "tester_user2"]:
            admin = (u == "tester_admin")
            payload = {"username": u, "password": "password123", "admin": admin}
            try:
                r = requests.post(f"{BASE_URL}/_matrix/client/r0/register", json=payload, timeout=5)
            except: pass
            
            login_payload = {"type": "m.login.password", "user": u, "password": "password123"}
            try:
                r = requests.post(f"{BASE_URL}/_matrix/client/r0/login", json=login_payload, timeout=5)
                if r.status_code == 200:
                    d = r.json()
                    self.tokens[u] = d["access_token"]
                    self.user_ids[u] = d["user_id"]
                    if u == "tester_user1":
                        self.data["user_id"] = d["user_id"]
                        self.data["device_id"] = d["device_id"]
            except: pass
        
        # Setup a room
        if "tester_user1" in self.tokens:
            headers = {"Authorization": f"Bearer {self.tokens['tester_user1']}"}
            r = requests.post(f"{BASE_URL}/_matrix/client/r0/createRoom", json={"name": "Test"}, headers=headers)
            if r.status_code == 200:
                self.data["room_id"] = r.json()["room_id"]
                r = requests.post(f"{BASE_URL}/_matrix/client/r0/rooms/{self.data['room_id']}/send/m.room.message", json={"msgtype": "m.text", "body": "init"}, headers=headers)
                if r.status_code == 200:
                    self.data["event_id"] = r.json()["event_id"]

    def parse_apis(self, md_path):
        apis = []
        with open(md_path, 'r') as f:
            lines = f.readlines()
        
        table_started = False
        for line in lines:
            if "| 方法 | 完整路径 |" in line:
                table_started = True
                continue
            if table_started and "|" in line and "---" not in line:
                parts = [p.strip() for p in line.split("|")]
                if len(parts) >= 6:
                    apis.append({
                        "method": parts[1],
                        "path": parts[2].strip("`"),
                        "auth": parts[3].strip("*"),
                        "module": parts[4],
                        "handler": parts[5]
                    })
        return apis

    def run_test(self, api):
        method = api["method"]
        path = api["path"]
        auth = api["auth"]
        
        url = f"{BASE_URL}{path}"
        for k, v in self.data.items():
            url = url.replace(f"{{{k}}}", str(v))
        
        headers = {}
        if "Admin" in auth:
            token = self.tokens.get('tester_admin')
            if token: headers["Authorization"] = f"Bearer {token}"
        elif "User" in auth or "Matrix" in auth:
            token = self.tokens.get('tester_user1')
            if token: headers["Authorization"] = f"Bearer {token}"

        payload = {}
        if "register" in path: payload = {"username": "newuser_" + str(time.time()), "password": "password"}
        elif "login" in path: payload = {"type": "m.login.password", "user": "tester_user1", "password": "password123"}
        elif "createRoom" in path: payload = {"name": "Test Room"}
        elif "send" in path: payload = {"msgtype": "m.text", "body": "Automated Test"}
        elif "upload" in path: payload = {"content": "SGVsbG8=", "content_type": "text/plain"}
        elif "private/sessions" in path: payload = {"other_user_id": self.user_ids.get("tester_user2", "@tester_user2:matrix.cjystx.top")}
        elif "friend/request" in path: payload = {"user_id": self.user_ids.get("tester_user2", "@tester_user2:matrix.cjystx.top")}
        elif "security/ip/block" in path: payload = {"ip_address": "1.2.3.4", "reason": "test"}
        elif "security/ip/unblock" in path: payload = {"ip_address": "1.2.3.4"}
        elif "admin" in path and "password" in path: payload = {"new_password": "new_password123"}

        try:
            if method == "GET":
                r = requests.get(url, headers=headers, timeout=5)
            elif method == "POST":
                r = requests.post(url, headers=headers, json=payload, timeout=5)
            elif method == "PUT":
                r = requests.put(url, headers=headers, json=payload, timeout=5)
            elif method == "DELETE":
                r = requests.delete(url, headers=headers, timeout=5)
            else:
                return "SKIPPED", "Unknown Method"

            if r.status_code < 400:
                return "OK", f"Code {r.status_code}"
            elif r.status_code == 401:
                return "FAIL", "401 Unauthorized"
            elif r.status_code == 403:
                return "FAIL", "403 Forbidden"
            elif r.status_code == 404:
                return "FAIL", "404 Not Found"
            elif r.status_code == 500:
                return "FAIL", "500 Server Error"
            else:
                return "WARN", f"Status {r.status_code}: {r.text[:50]}"
        except Exception as e:
            return "ERROR", str(e)

    def start(self):
        self.setup_accounts()
        apis = self.parse_apis("/home/hula/synapse_rust/docs/synapse-rust/api-reference.md")
        
        report = []
        for api in apis:
            print(f"Testing {api['method']} {api['path']}...")
            status, note = self.run_test(api)
            report.append({**api, "status": status, "note": note})
        
        self.update_markdown("/home/hula/synapse_rust/docs/synapse-rust/api-reference.md", report)

    def update_markdown(self, path, report):
        with open(path, 'r') as f:
            lines = f.readlines()
        
        new_lines = []
        table_header_found = False
        for line in lines:
            if "| 方法 | 完整路径 | 鉴权要求 | 业务模块 | 处理函数 | 测试状态 | 备注 |" in line:
                new_lines.append(line)
                table_header_found = True
                continue
            if table_header_found and "| :--- | :--- | :--- | :--- | :--- |" in line:
                new_lines.append(line)
                continue
            
            matched = False
            if table_header_found and "|" in line:
                parts = [p.strip() for p in line.split("|")]
                if len(parts) >= 8:
                    m = parts[1]
                    p = parts[2].strip("`")
                    for entry in report:
                        if entry["method"] == m and entry["path"] == p:
                            status_emoji = "✅" if entry["status"] == "OK" else "❌" if entry["status"] == "FAIL" else "⚠️"
                            new_lines.append(f"| {m} | `{p}` | **{entry['auth']}** | {entry['module']} | `{entry['handler']}` | {status_emoji} {entry['status']} | {entry['note']} |\n")
                            matched = True
                            break
            
            if not matched:
                new_lines.append(line)

        with open(path, 'w') as f:
            f.writelines(new_lines)

if __name__ == "__main__":
    tester = MatrixTester()
    tester.start()
