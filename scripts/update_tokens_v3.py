#!/usr/bin/env python3
"""
更新所有测试脚本中的token
"""

import json
import os
import re

new_testuser1_token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTg0MDUwLCJpYXQiOjE3NzAxODA0NTAsImRldmljZV9pZCI6Ik4zbUhuam1ZWFhxZ3VBZGgifQ.G8092HdzmY_a73l-jvzYBsLTd4TLf2PVOkdkDwAy2X8"

new_testuser2_token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDE4NDA1MCwiaWF0IjoxNzcwMTgwNDUwLCJkZXZpY2VfaWQiOiJWZmJaaHdISnNvTUVwcVFqIn0.9oeTsXKIEv6y_ZiTMdZk3oB1llqUxEHEm4gw2qqQ6ss"

new_admin_token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc3MDE3ODQ4NiwiaWF0IjoxNzcwMTc0ODg2LCJkZXZpY2VfaWQiOiJUYmNDOW9Dd3Fhd2pZQWZvIn0.P_fKcVGQJCPrUZ14p4kiRO80PNDv-0YQ-6H-_hT5COo"

print(f"new_testuser1_token: {new_testuser1_token}")
print(f"new_testuser2_token: {new_testuser2_token}")
print(f"new_admin_token: {new_admin_token}")
print()

old_testuser1_tokens = set()
old_testuser2_tokens = set()
old_admin_tokens = set()

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
            
            testuser1_pattern = r'["\']access_token["\']?\s*:\s*["\']([^"\']+)["\']'
            for match in re.finditer(testuser1_pattern, content):
                token = match.group(1)
                if token.startswith('eyJ') and '@testuser1:' in content[max(0, match.start()-100):match.start()]:
                    old_testuser1_tokens.add(token)
                elif token.startswith('eyJ') and '@testuser2:' in content[max(0, match.start()-100):match.start()]:
                    old_testuser2_tokens.add(token)
                elif token.startswith('eyJ') and '@admin:' in content[max(0, match.start()-100):match.start()]:
                    old_admin_tokens.add(token)

print(f"找到 testuser1 的旧token: {len(old_testuser1_tokens)} 个")
print(f"找到 testuser2 的旧token: {len(old_testuser2_tokens)} 个")
print(f"找到 admin 的旧token: {len(old_admin_tokens)} 个")
print()

for script in test_scripts:
    if os.path.exists(script):
        with open(script, 'r', encoding='utf-8') as f:
            content = f.read()
        
        original_content = content
        
        for old_token in old_testuser1_tokens:
            content = content.replace(old_token, new_testuser1_token)
        
        for old_token in old_testuser2_tokens:
            content = content.replace(old_token, new_testuser2_token)
        
        for old_token in old_admin_tokens:
            content = content.replace(old_token, new_admin_token)
        
        if content != original_content:
            with open(script, 'w', encoding='utf-8') as f:
                f.write(content)
            print(f"✅ 更新完成: {script}")
        else:
            print(f"⏭️  无需更新: {script}")
    else:
        print(f"❌ 文件不存在: {script}")

print("\n所有测试脚本token更新完成！")
