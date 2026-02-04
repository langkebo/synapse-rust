#!/usr/bin/env python3
"""
更新所有测试脚本中的token
"""

import json
import os

# 获取最新的token
response = os.popen('curl -X POST http://localhost:8008/_matrix/client/r0/login -H "Content-Type: application/json" -d \'{"type":"m.login.password","user":"@testuser1:matrix.cjystx.top","password":"TestUser123456!"}\'').read()
data = json.loads(response)
testuser1_token = data.get('access_token', '')

response = os.popen('curl -X POST http://localhost:8008/_matrix/client/r0/login -H "Content-Type: application/json" -d \'{"type":"m.login.password","user":"@testuser2:matrix.cjystx.top","password":"TestUser123456!"}\'').read()
data = json.loads(response)
testuser2_token = data.get('access_token', '')

response = os.popen('curl -X POST http://localhost:8008/_matrix/client/r0/login -H "Content-Type: application/json" -d \'{"type":"m.login.password","user":"@admin:matrix.cjystx.top","password":"Wzc9890951!"}\'').read()
data = json.loads(response)
admin_token = data.get('access_token', '')

print(f"testuser1_token: {testuser1_token}")
print(f"testuser2_token: {testuser2_token}")
print(f"admin_token: {admin_token}")

# 更新测试脚本
test_scripts = [
    "scripts/test_core_client_api.py",
    "scripts/test_admin_api.py",
    "scripts/test_e2e_encryption_api.py",
    "scripts/test_voice_message_api.py",
    "scripts/test_friend_system_api.py",
    "scripts/test_media_file_api.py",
    "scripts/test_private_chat_api.py",
    "scripts/test_key_backup_api.py",
    "scripts/test_authentication_error_handling.py",
]

for script in test_scripts:
    if os.path.exists(script):
        with open(script, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # 更新testuser1的token
        content = content.replace('eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTY0NzAxLCJpYXQiOjE3NzAxNjExMDEsImRldmljZV9pZCI6IklXSVNlSW84SVRCY3JxdEMifQ.fdut9dkP9xaSS8iQM83EUvXJnxy8Phg4t9OpduydHpY', testuser1_token)
        
        # 更新testuser2的token
        content = content.replace('eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDE3MDE4NSwiaWF0IjoxNzcwMTY2NTg1LCJkZXZpY2VfaWQiOiJLRkFkejR0cVRPblZwT2h5In0.zcZYm-k7Rl4MHj_sC7nMdgHtu5Cjf24f5fMFt6BYMxg', testuser2_token)
        
        # 更新admin的token
        content = content.replace('eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkBhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTY3NDI1LCJpYXQiOjE3NzAxNjM4MjUsImRldmljZV9pZCI6Ildhb1NU1RrblhVUWVOUXliSGsifQ.GEv6PcxkxV9W0YPu9I8nKZVDMxTxkftbAoyAAuJ9ja4', admin_token)
        
        with open(script, 'w', encoding='utf-8') as f:
            f.write(content)
        
        print(f"✅ 更新完成: {script}")
    else:
        print(f"❌ 文件不存在: {script}")

print("\n所有测试脚本token更新完成！")
