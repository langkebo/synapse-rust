#!/usr/bin/env python3
"""
更新所有测试脚本中的token
"""

import json
import os
import re

# 新的有效token
testuser1_token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMTcyNDQ5LCJpYXQiOjE3NzAxNjg4NDksImRldmljZV9pZCI6InVtY1FPd2xQcktmQXNUSmwifQ.KiLXtCMTLDfjYgdjYiWWz0kseQl3dZ0tXo9MO2urobQ"

testuser2_token = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDE3MjQ3MiwiaWF0IjoxNzcwMTY4ODcyLCJkZXZpY2VfaWQiOiJFWXBrT2NKckhCUDdGSEh2In0.bqdJEYfZ0zQl9SpnEXpdkRMZvEg1_VVxF_JOnQopKv4"

admin_token = testuser1_token  # testuser1是管理员

print(f"testuser1_token: {testuser1_token}")
print(f"testuser2_token: {testuser2_token}")
print(f"admin_token: {admin_token}")
print()

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

# 读取所有旧token
old_tokens = set()
for script in test_scripts:
    if os.path.exists(script):
        with open(script, 'r', encoding='utf-8') as f:
            content = f.read()
            # 查找所有access_token
            tokens = re.findall(r'access_token["\s:=]+["\']([^"\']+)["\']', content)
            for token in tokens:
                if token.startswith('eyJ'):
                    old_tokens.add(token)

print(f"找到 {len(old_tokens)} 个旧token")
print()

# 更新测试脚本
for script in test_scripts:
    if os.path.exists(script):
        with open(script, 'r', encoding='utf-8') as f:
            content = f.read()
        
        original_content = content
        
        # 更新testuser1的token
        for old_token in old_tokens:
            content = content.replace(old_token, testuser1_token)
        
        # 如果内容有变化，写回文件
        if content != original_content:
            with open(script, 'w', encoding='utf-8') as f:
                f.write(content)
            print(f"✅ 更新完成: {script}")
        else:
            print(f"⏭️  无需更新: {script}")
    else:
        print(f"❌ 文件不存在: {script}")

print("\n所有测试脚本token更新完成！")
