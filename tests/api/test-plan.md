# Synapse Matrix Server API 测试方案

## 1. 测试目标

全面测试 Synapse Matrix Server 的所有 API 端点，确保功能正确性、性能稳定性和安全性。

## 2. 测试环境

### 2.1 服务器信息
- 服务器地址: http://localhost:8008
- 服务器名称: cjystx.top
- 版本: 0.1.0

### 2.2 测试工具
- curl: HTTP 请求工具
- jq: JSON 处理工具
- bash: 测试脚本执行环境

## 3. 测试策略

### 3.1 测试分类

#### 3.1.1 功能测试
- 正向测试: 验证正常输入的正确响应
- 负向测试: 验证错误输入的错误处理
- 边界测试: 验证边界条件的处理

#### 3.1.2 性能测试
- 响应时间测试: 验证 API 响应时间
- 并发测试: 验证并发请求处理能力
- 压力测试: 验证系统在高负载下的表现

#### 3.1.3 安全测试
- 认证测试: 验证认证机制
- 授权测试: 验证权限控制
- 输入验证测试: 验证输入过滤和验证
- SQL 注入测试: 验证 SQL 注入防护
- XSS 测试: 验证跨站脚本防护

### 3.2 测试优先级

#### 高优先级 (P0)
- 用户注册与登录
- 消息发送与接收
- 房间创建与管理
- 联邦通信

#### 中优先级 (P1)
- 用户资料管理
- 设备管理
- 在线状态
- 媒体上传

#### 低优先级 (P2)
- 搜索功能
- 推送通知
- 高级功能

## 4. 测试用例设计

### 4.1 基础服务 API (7 个端点)

#### 4.1.1 健康检查
```bash
# 测试用例 1: 健康检查
curl -s http://localhost:8008/health | jq .
# 预期: {"status":"healthy",...}

# 测试用例 2: 版本信息
curl -s http://localhost:8008/_matrix/client/versions | jq .
# 预期: {"versions":["r0.5.0","r0.6.0",...],...}

# 测试用例 3: 服务器发现
curl -s http://localhost:8008/.well-known/matrix/server | jq .
# 预期: {"m.server":"cjystx.top:8008"}
```

### 4.2 用户注册与认证 API (8 个端点)

#### 4.2.1 用户注册
```bash
# 测试用例 1: 正常注册
curl -X POST http://localhost:8008/_matrix/client/r0/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser",
    "password": "Test@123",
    "auth": {"type": "m.login.dummy"}
  }' | jq .
# 预期: {"access_token":"...","user_id":"@testuser:cjystx.top",...}

# 测试用例 2: 重复注册
curl -X POST http://localhost:8008/_matrix/client/r0/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser",
    "password": "Test@123",
    "auth": {"type": "m.login.dummy"}
  }' | jq .
# 预期: {"error":"Username already taken","errcode":"M_USER_IN_USE"}

# 测试用例 3: 弱密码
curl -X POST http://localhost:8008/_matrix/client/r0/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser_weak",
    "password": "123",
    "auth": {"type": "m.login.dummy"}
  }' | jq .
# 预期: {"error":"Password too weak","errcode":"M_BAD_JSON"}
```

#### 4.2.2 用户登录
```bash
# 测试用例 1: 正常登录
curl -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{
    "type": "m.login.password",
    "user": "testuser",
    "password": "Test@123"
  }' | jq .
# 预期: {"access_token":"...","user_id":"@testuser:cjystx.top",...}

# 测试用例 2: 错误密码
curl -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{
    "type": "m.login.password",
    "user": "testuser",
    "password": "wrongpassword"
  }' | jq .
# 预期: {"error":"Invalid credentials","errcode":"M_FORBIDDEN"}

# 测试用例 3: 不存在的用户
curl -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{
    "type": "m.login.password",
    "user": "nonexistent",
    "password": "Test@123"
  }' | jq .
# 预期: {"error":"User not found","errcode":"M_FORBIDDEN"}
```

### 4.3 房间管理 API (10 个端点)

#### 4.3.1 创建房间
```bash
# 测试用例 1: 创建公开房间
curl -X POST http://localhost:8008/_matrix/client/r0/createRoom \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Room",
    "visibility": "public",
    "preset": "public_chat"
  }' | jq .
# 预期: {"room_id":"!xxx:cjystx.top"}

# 测试用例 2: 创建私有房间
curl -X POST http://localhost:8008/_matrix/client/r0/createRoom \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Private Room",
    "visibility": "private",
    "preset": "private_chat"
  }' | jq .
# 预期: {"room_id":"!xxx:cjystx.top"}

# 测试用例 3: 无认证创建房间
curl -X POST http://localhost:8008/_matrix/client/r0/createRoom \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Test Room",
    "visibility": "public"
  }' | jq .
# 预期: {"error":"Missing token","errcode":"M_MISSING_TOKEN"}
```

#### 4.3.2 加入房间
```bash
# 测试用例 1: 加入公开房间
curl -X POST http://localhost:8008/_matrix/client/r0/join/!roomId:cjystx.top \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}' | jq .
# 预期: {"room_id":"!roomId:cjystx.top"}

# 测试用例 2: 加入私有房间(无权限)
curl -X POST http://localhost:8008/_matrix/client/r0/join/!privateRoomId:cjystx.top \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}' | jq .
# 预期: {"error":"You are not invited to this room","errcode":"M_FORBIDDEN"}
```

### 4.4 消息发送与接收 API (8 个端点)

#### 4.4.1 发送消息
```bash
# 测试用例 1: 发送文本消息
curl -X PUT "http://localhost:8008/_matrix/client/r0/rooms/!roomId:cjystx.top/send/m.room.message/txn123" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "msgtype": "m.text",
    "body": "Hello, World!"
  }' | jq .
# 预期: {"event_id":"$eventId"}

# 测试用例 2: 发送空消息
curl -X PUT "http://localhost:8008/_matrix/client/r0/rooms/!roomId:cjystx.top/send/m.room.message/txn124" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "msgtype": "m.text",
    "body": ""
  }' | jq .
# 预期: {"error":"Message body cannot be empty","errcode":"M_BAD_JSON"}

# 测试用例 3: 发送到不存在的房间
curl -X PUT "http://localhost:8008/_matrix/client/r0/rooms/!nonexistent:cjystx.top/send/m.room.message/txn125" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "msgtype": "m.text",
    "body": "Test"
  }' | jq .
# 预期: {"error":"Room not found","errcode":"M_NOT_FOUND"}
```

### 4.5 媒体管理 API (6 个端点)

#### 4.5.1 上传媒体
```bash
# 测试用例 1: 上传图片
curl -X POST http://localhost:8008/_matrix/media/r0/upload \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: image/jpeg" \
  --data-binary "@test.jpg" | jq .
# 预期: {"content_uri":"mxc://cjystx.top/xxx"}

# 测试用例 2: 上传超大文件
curl -X POST http://localhost:8008/_matrix/media/r0/upload \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@large_file.bin" | jq .
# 预期: {"error":"File too large","errcode":"M_TOO_LARGE"}
```

### 4.6 联邦 API (5 个端点)

#### 4.6.1 服务器版本
```bash
# 测试用例 1: 获取服务器版本
curl -s http://localhost:8008/_matrix/federation/v1/version | jq .
# 预期: {"server":{"name":"Synapse Rust","version":"0.1.0"}}
```

#### 4.6.2 服务器密钥
```bash
# 测试用例 1: 获取服务器密钥
curl -s http://localhost:8008/_matrix/key/v2/server | jq .
# 预期: {"server_name":"cjystx.top","valid_until_ts":...}
```

## 5. 测试执行计划

### 5.1 测试阶段

#### 阶段 1: 基础功能测试 (第 1-2 天)
- 基础服务 API 测试
- 用户注册与认证 API 测试
- 账户管理 API 测试

#### 阶段 2: 核心功能测试 (第 3-4 天)
- 房间管理 API 测试
- 消息发送与接收 API 测试
- 设备管理 API 测试

#### 阶段 3: 高级功能测试 (第 5-6 天)
- 媒体管理 API 测试
- 在线状态 API 测试
- 用户目录 API 测试

#### 阶段 4: 联邦与安全测试 (第 7-8 天)
- 联邦 API 测试
- 安全测试
- 性能测试

### 5.2 测试脚本

创建自动化测试脚本，按模块执行测试:

```bash
# 执行所有测试
./run_all_tests.sh

# 执行特定模块测试
./test_module.sh basic
./test_module.sh auth
./test_module.sh rooms
./test_module.sh messages
./test_module.sh media
./test_module.sh federation
```

## 6. 测试结果记录

### 6.1 测试结果模板

| 测试用例 ID | 测试项 | 预期结果 | 实际结果 | 状态 | 备注 |
|------------|--------|----------|----------|------|------|
| TC001 | 用户注册 | 返回 access_token | - | - | - |
| TC002 | 用户登录 | 返回 access_token | - | - | - |

### 6.2 缺陷记录模板

| 缺陷 ID | 标题 | 严重程度 | 优先级 | 状态 | 描述 | 重现步骤 |
|---------|------|----------|--------|------|------|----------|
| BUG001 | - | - | - | - | - | - |

## 7. 测试报告

### 7.1 测试总结
- 测试用例总数: 383
- 通过用例数: -
- 失败用例数: -
- 阻塞用例数: -
- 测试覆盖率: -

### 7.2 缺陷统计
- 严重缺陷: -
- 一般缺陷: -
- 轻微缺陷: -
- 总计: -

### 7.3 建议
- 功能改进建议
- 性能优化建议
- 安全加固建议

## 8. 测试环境维护

### 8.1 环境重置
```bash
# 重置测试环境
./reset_test_env.sh

# 清理测试数据
./cleanup_test_data.sh
```

### 8.2 环境监控
```bash
# 检查服务状态
docker compose ps

# 查看服务日志
docker compose logs synapse-rust

# 检查数据库连接
docker compose exec db psql -U synapse -d synapse_test -c "SELECT 1"
```

## 9. 附录

### 9.1 测试数据
- 测试账户列表
- 测试房间列表
- 测试媒体文件

### 9.2 参考文档
- Matrix Client-Server API 规范
- Matrix Federation API 规范
- Synapse 官方文档
