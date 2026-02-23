# 管理员账号注册指南

本文档说明如何在 Synapse Rust 中注册管理员账号。

## 注册流程

管理员注册使用 HMAC-SHA256 签名验证机制，确保只有知道共享密钥（shared_secret）的用户才能注册管理员账号。

### 步骤

1. **获取 nonce**
   ```bash
   curl -X GET http://localhost:8008/_synapse/admin/v1/register/nonce
   ```

   响应示例：
   ```json
   {
     "nonce": "EvLl3H2_TaayB4kiG1rowvdokrG4qusJ-jXc1xnr5PmDUSgb3OURPUyRHDeY34PFKj9ps3CAsbgNhyRN5dDPkQ"
   }
   ```

2. **计算 HMAC-SHA256**
   
   HMAC 计算格式：
   ```
   HMAC-SHA256(shared_secret, nonce + "\0" + username + "\0" + password + "\0" + "admin"/"notadmin" + ("\0" + user_type if user_type exists))
   ```

   Python 示例：
   ```python
   import hmac
   import hashlib
   
   shared_secret = "test_shared_secret"
   nonce = "EvLl3H2_TaayB4kiG1rowvdokrG4qusJ-jXc1xnr5PmDUSgb3OURPUyRHDeY34PFKj9ps3CAsbgNhyRN5dDPkQ"
   username = "admin"
   password = "Wzc9890951!"
   admin = True
   user_type = None
   
   # 构建消息
   message = nonce.encode('utf-8')
   message += b'\x00'
   message += username.encode('utf-8')
   message += b'\x00'
   message += password.encode('utf-8')
   message += b'\x00'
   message += b'admin' if admin else b'notadmin'
   
   # 只有当user_type存在时才添加
   if user_type:
       message += b'\x00'
       message += user_type.encode('utf-8')
   
   # 计算HMAC
   key = shared_secret.encode('utf-8')
   mac = hmac.new(key, message, hashlib.sha256)
   mac_hex = mac.hexdigest()
   ```

3. **注册管理员账号**
   ```bash
   curl -X POST http://localhost:8008/_synapse/admin/v1/register \
     -H "Content-Type: application/json" \
     -d '{
       "nonce": "EvLl3H2_TaayB4kiG1rowvdokrG4qusJ-jXc1xnr5PmDUSgb3OURPUyRHDeY34PFKj9ps3CAsbgNhyRN5dDPkQ",
       "username": "admin",
       "password": "Wzc9890951!",
       "admin": true,
       "displayname": "System Administrator",
       "mac": "635cd95a1c52395f6b9cacfb2d5d9bf0ffaaca90c92771616b758606c7e57e81"
     }'
   ```

   响应示例：
   ```json
   {
     "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
     "refresh_token": "KijjldEFrnHEaHVrctMv4h1m0BtbjpU1HbamCA35ZK4",
     "expires_in": 3600,
     "device_id": "q5Gm1PW0Kr_j0rliSTRWSw",
     "user_id": "@admin:matrix.cjystx.top",
     "home_server": "matrix.cjystx.top"
   }
   ```

## 配置要求

在 `docker/config/homeserver.yaml` 中启用管理员注册：

```yaml
admin_registration:
  enabled: true
  shared_secret: "test_shared_secret"
  nonce_timeout_seconds: 60
```

**重要**：
- `enabled` 必须设置为 `true`
- `shared_secret` 是用于 HMAC 计算的密钥，必须保密
- `nonce_timeout_seconds` 是 nonce 的有效期，默认 60 秒

## 自动化脚本

项目提供了自动化注册脚本：`scripts/register_admin.py`

使用方法：
```bash
python3 scripts/register_admin.py
```

脚本会自动完成以下步骤：
1. 获取 nonce
2. 计算 HMAC-SHA256
3. 注册管理员账号
4. 显示注册结果

## 测试管理员权限

注册成功后，可以使用管理员 Token 访问管理员 API：

```bash
# 获取服务器状态
curl -X GET http://localhost:8008/_synapse/admin/v1/status \
  -H "Authorization: Bearer {admin_access_token}"

# 获取用户列表
curl -X GET "http://localhost:8008/_synapse/admin/v1/users?limit=10&offset=0" \
  -H "Authorization: Bearer {admin_access_token}"

# 获取房间列表
curl -X GET "http://localhost:8008/_synapse/admin/v1/rooms?limit=10&offset=0" \
  -H "Authorization: Bearer {admin_access_token}"
```

## 安全注意事项

1. **保护 shared_secret**：不要将 shared_secret 提交到版本控制系统
2. **使用强密码**：管理员密码应包含大小写字母、数字和特殊字符
3. **定期更换密码**：建议定期更换管理员密码
4. **限制访问**：在生产环境中，应该限制管理员注册 API 的访问
5. **审计日志**：监控管理员注册和操作日志

## 常见错误

### 1. Admin registration is not enabled

**原因**：管理员注册功能未启用

**解决**：在 `homeserver.yaml` 中设置 `admin_registration.enabled: true`

### 2. Unrecognised nonce

**原因**：nonce 无效或已过期

**解决**：重新获取 nonce 并在有效期内使用

### 3. HMAC incorrect

**原因**：HMAC 计算不正确

**解决**：
- 检查 shared_secret 是否正确
- 确认 HMAC 计算格式是否正确
- 注意 null 字节 `\x00` 的处理
- 检查 `admin` 和 `notadmin` 的选择

### 4. Shared secret is not configured

**原因**：shared_secret 未配置或为空

**解决**：在 `homeserver.yaml` 中设置 `admin_registration.shared_secret`

## 当前测试账号

| 账号类型 | 用户ID | 密码 | 设备ID | Access Token |
|---------|--------|------|--------|-------------|
| 管理员 | `@admin:matrix.cjystx.top` | `Wzc9890951!` | `q5Gm1PW0Kr_j0rliSTRWSw` | `eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc3MDEzMjMyMSwiaWF0IjoxNzcwMTI4NzIxLCJkZXZpY2VfaWQiOiJxNUdtMVBXMEtyX2owcmxpU1RSV1N3In0.gAHe9KBK5nPA6LQ7V9zt2UdpTQHp-9CuJC47uWj6FGI` |

**注意**：Access Token 有效期为 1 小时，过期后需要重新登录或使用 refresh_token 刷新。
