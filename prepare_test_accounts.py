
import hmac
import hashlib
import json
import urllib.request
import urllib.error
SERVER = "http://localhost:28008"
ADMIN_SHARED_SECRET = "15c7a2cfdeb364a8d6d0dd8fa45ef45209021023c4b7937f6c3887c0788b155d"

def get_nonce():
    with urllib.request.urlopen(f"{SERVER}/_synapse/admin/v1/register/nonce") as resp:
        return json.loads(resp.read())["nonce"]

def calculate_mac(secret, nonce, username, password, admin, user_type=None):
    message = bytearray()
    message.extend(nonce.encode('utf-8'))
    message.extend(b'\x00')
    message.extend(username.encode('utf-8'))
    message.extend(b'\x00')
    message.extend(password.encode('utf-8'))
    message.extend(b'\x00')
    message.extend(b'admin\x00\x00\x00' if admin else b'notadmin')
    if user_type:
        message.extend(b'\x00')
        message.extend(user_type.encode('utf-8'))
    key = secret.encode('utf-8')
    mac = hmac.new(key, bytes(message), hashlib.sha256)
    return mac.hexdigest()

def register(username, password, is_admin, user_type=None):
    nonce = get_nonce()
    mac = calculate_mac(
        ADMIN_SHARED_SECRET, nonce, username, password, is_admin, user_type
    )
    data = {
        "nonce": nonce,
        "username": username,
        "password": password,
        "admin": is_admin,
        "displayname": username.replace("_", " ").title(),
        "mac": mac
    }
    if user_type:
        data["user_type"] = user_type
    req = urllib.request.Request(
        f"{SERVER}/_synapse/admin/v1/register",
        data=json.dumps(data).encode('utf-8'),
        headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        print(f"Error registering {username}: {e.read().decode()}")
        return None

def login(username, password):
    data = {
        "type": "m.login.password",
        "user": username,
        "password": password
    }
    req = urllib.request.Request(
        f"{SERVER}/_matrix/client/v3/login",
        data=json.dumps(data).encode('utf-8'),
        headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        print(f"Error logging in {username}: {e.read().decode()}")
        return None

users = [
    ("qa_super_admin", "Test@123", True, "super_admin"),
    ("qa_super_admin_ops", "Test@123", True, "super_admin"),
    ("qa_admin", "Test@123", True, "admin"),
    ("qa_admin_ops", "Test@123", True, "admin"),
    ("qa_user", "Test@123", False, None),
]

tokens = {}
for username, password, is_admin, user_type in users:
    print(f"Processing {username}...")
    reg_res = register(username, password, is_admin, user_type)
    if reg_res:
        tokens[username] = reg_res["access_token"]
    else:
        # Try login if already registered
        login_res = login(username, password)
        if login_res:
            tokens[username] = login_res["access_token"]

with open("test_tokens.json", "w") as f:
    json.dump(tokens, f, indent=2)

print("Tokens saved to test_tokens.json")
