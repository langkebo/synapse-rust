#!/usr/bin/env python3
import hashlib
import hmac
import json
import requests
import sys

SHARED_SECRET = "test_shared_secret"
USERNAME = "admin"
PASSWORD = "AdminPass123456!"
ADMIN_FLAG = True
BASE_URL = "http://localhost:8008"

def get_nonce():
    """Get nonce for registration"""
    url = f"{BASE_URL}/_synapse/admin/v1/register/nonce"
    response = requests.get(url)
    if response.status_code == 200:
        return response.json().get("nonce")
    else:
        print(f"Failed to get nonce: {response.status_code} - {response.text}")
        sys.exit(1)

def calculate_mac(nonce, username, password, admin_flag):
    """Calculate HMAC-SHA256 MAC"""
    message = f"{nonce}\x00{username}\x00{password}\x00{'admin' if admin_flag else 'notadmin'}"
    key = SHARED_SECRET.encode('utf-8')
    mac = hmac.new(key, message.encode('utf-8'), hashlib.sha256)
    return mac.hexdigest()

def register_admin():
    """Register admin user"""
    print("Getting nonce from server...")
    nonce = get_nonce()
    print(f"Nonce obtained: {nonce}")
    
    print(f"Registering admin user: {USERNAME}")
    mac = calculate_mac(nonce, USERNAME, PASSWORD, ADMIN_FLAG)
    
    url = f"{BASE_URL}/_synapse/admin/v1/register"
    registration_data = {
        "nonce": nonce,
        "username": USERNAME,
        "password": PASSWORD,
        "admin": ADMIN_FLAG,
        "displayname": "System Administrator",
        "mac": mac
    }
    
    response = requests.post(
        url,
        json=registration_data,
        headers={"Content-Type": "application/json"}
    )
    
    print(f"Registration response status: {response.status_code}")
    print(f"Registration response: {response.text}")
    
    if response.status_code in [200, 201]:
        result = response.json()
        print("\n=== Admin Registration Successful ===")
        print(f"User ID: {result.get('user_id')}")
        print(f"Access Token: {result.get('access_token')}")
        print(f"Home Server: {result.get('home_server')}")
        
        # Save token to file
        with open('/home/hula/synapse_rust/admin_token.txt', 'w') as f:
            f.write(result.get('access_token'))
        print("\nToken saved to /home/hula/synapse_rust/admin_token.txt")
        return result
    else:
        print("\n=== Registration Failed ===")
        sys.exit(1)

if __name__ == "__main__":
    register_admin()
