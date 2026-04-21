# API 契约文档更新指南

> 日期: 2026-04-15
> 目标: 系统性更新 matrix-js-sdk API 契约文档

## 快速开始

### 更新单个模块的步骤

1. **找到后端路由文件**
   ```bash
   # 例如更新 auth.md
   cd /Users/ljf/Desktop/hu/synapse-rust
   cat src/web/routes/auth.rs
   ```

2. **提取路由定义**
   ```bash
   # 查找所有 .route() 调用
   grep -n "\.route(" src/web/routes/auth.rs
   
   # 查找处理器函数
   grep -n "async fn" src/web/routes/auth.rs
   ```

3. **查看处理器实现**
   ```bash
   # 查看具体的请求/响应结构
   grep -A 20 "async fn login" src/web/routes/auth.rs
   ```

4. **更新契约文档**
   ```bash
   cd /Users/ljf/Desktop/hu/matrix-js-sdk/docs/api-contract
   vim auth.md
   ```

## 关键文件位置

### 后端路由文件
```
synapse-rust/src/web/routes/
├── assembly.rs          # 主路由装配
├── admin/
│   └── mod.rs          # 管理员路由
├── auth.rs             # 认证路由
├── account.rs          # 账户路由
├── device.rs           # 设备路由
├── dm.rs               # DM 路由
├── e2ee_routes.rs      # E2EE 路由
├── federation.rs       # 联邦路由
├── friend_room.rs      # 好友路由
├── key_backup.rs       # 密钥备份
├── media.rs            # 媒体路由
├── presence.rs         # 在线状态
├── push.rs             # 推送路由
├── rendezvous.rs       # Rendezvous
├── room.rs             # 房间路由
├── room_summary.rs     # 房间摘要
├── space.rs            # 空间路由
├── sync.rs             # 同步路由
├── sliding_sync.rs     # Sliding Sync
├── verification_routes.rs  # 设备验证
├── voice.rs            # 语音路由
├── widget.rs           # Widget 路由
└── handlers/
    ├── room.rs         # 房间处理器
    └── thread.rs       # 线程处理器
```

### 契约文档文件
```
matrix-js-sdk/docs/api-contract/
├── auth.md
├── admin.md
├── device.md
├── dm.md
├── e2ee.md
├── federation.md
├── friend.md
├── key-backup.md
├── media.md
├── presence.md
├── push.md
├── rendezvous.md
├── room.md
├── room-summary.md
├── space.md
├── sync.md
├── thread.md
├── verification.md
├── voice.md
└── widget.md
```

## 更新模板

### 基本端点格式

```markdown
### 端点名称

**路径**: `/_matrix/client/v3/endpoint/{param}`
**方法**: POST
**版本**: v3
**认证**: 用户认证

#### 请求参数

**路径参数**:
- `param` (string, 必需): 参数说明

**查询参数**:
- `limit` (integer, 可选, 默认: 10): 返回数量限制
- `offset` (integer, 可选, 默认: 0): 偏移量

**请求体**:
\`\`\`json
{
  "field1": "string",
  "field2": 123,
  "field3": {
    "nested": "value"
  }
}
\`\`\`

**字段说明**:
- `field1` (string, 必需): 字段说明
- `field2` (integer, 可选): 字段说明
- `field3` (object, 可选): 嵌套对象

#### 响应

**成功响应 (200)**:
\`\`\`json
{
  "result": "success",
  "data": {}
}
\`\`\`

**字段说明**:
- `result` (string): 结果状态
- `data` (object): 返回数据

**错误响应**:
- `400 Bad Request` - 请求参数错误
- `401 Unauthorized` - 未认证
- `403 Forbidden` - 权限不足
- `404 Not Found` - 资源不存在
- `429 Too Many Requests` - 请求过于频繁

#### 示例

**请求**:
\`\`\`bash
curl -X POST "https://matrix.example.com/_matrix/client/v3/endpoint/test" \\
  -H "Authorization: Bearer ACCESS_TOKEN" \\
  -H "Content-Type: application/json" \\
  -d '{"field1": "value"}'
\`\`\`

**响应**:
\`\`\`json
{
  "result": "success"
}
\`\`\`
```

## 常用命令

### 查找路由定义
```bash
# 查找所有 POST 路由
grep -r "post(" src/web/routes/ | grep "\.route"

# 查找特定路径
grep -r "/_matrix/client/v3/login" src/web/routes/

# 查找处理器函数
grep -r "async fn login" src/web/routes/
```

### 查找请求/响应结构
```bash
# 查找 DTO 定义
grep -r "struct.*Request" src/web/routes/
grep -r "struct.*Response" src/web/routes/

# 查找 JSON 响应
grep -r "Json(json!" src/web/routes/
```

### 查找认证要求
```bash
# 查找认证提取器
grep -r "AuthenticatedUser" src/web/routes/
grep -r "AdminUser" src/web/routes/
grep -r "OptionalAuthenticatedUser" src/web/routes/
```

## 验证清单

更新每个文档后，检查：

- [ ] 所有路径是否正确
- [ ] HTTP 方法是否正确
- [ ] API 版本是否正确
- [ ] 认证要求是否正确
- [ ] 请求参数是否完整
- [ ] 参数类型是否正确
- [ ] 参数约束是否准确
- [ ] 响应结构是否完整
- [ ] 状态码是否完整
- [ ] 错误码是否准确
- [ ] 示例是否有效

## 常见问题

### Q: 如何确定 API 版本？
A: 查看路由定义中的路径前缀：
- `/_matrix/client/r0/` → r0
- `/_matrix/client/v1/` → v1
- `/_matrix/client/v3/` → v3

### Q: 如何确定认证要求？
A: 查看处理器函数签名：
- `AuthenticatedUser` → 需要用户认证
- `AdminUser` → 需要管理员认证
- `OptionalAuthenticatedUser` → 可选认证
- 无认证提取器 → 公开端点

### Q: 如何找到请求/响应结构？
A: 查看处理器函数：
1. 函数参数中的 `Json<T>` → 请求体类型
2. 函数返回值中的 `Json<T>` → 响应体类型
3. 查找 `json!()` 宏 → 内联 JSON 响应

### Q: 如何处理多版本路由？
A: 分别列出每个版本的路由，不要合并：
```markdown
| POST | `/_matrix/client/r0/login` | r0 | ... |
| POST | `/_matrix/client/v3/login` | v3 | ... |
```

## 优先级建议

### 第一批 (核心 API)
1. auth.md - 认证是基础
2. room.md - 房间是核心
3. sync.md - 同步是关键
4. e2ee.md - 加密是安全
5. media.md - 媒体是常用

### 第二批 (重要功能)
6. admin.md - 管理功能
7. device.md - 设备管理
8. push.md - 推送通知
9. dm.md - 直接消息
10. presence.md - 在线状态

### 第三批 (扩展功能)
11-27. 其他文档

## 自动化工具

### 路由提取脚本
```bash
# 提取所有路由
./scripts/extract_routes.sh

# 查看提取结果
cat /tmp/routes_*.txt
```

### 验证脚本
```bash
# 验证文档与代码一致性
./scripts/verify_api_contract.sh
```

## 提交规范

更新文档后，使用以下提交信息格式：

```
docs: update [module] API contract

Updated [module].md to match backend implementation:
- [新增] Added new endpoint: POST /path
- [修改] Updated request parameters for GET /path
- [修复] Fixed response structure for PUT /path
- [移除] Removed deprecated endpoint: DELETE /path

Verified against: synapse-rust/src/web/routes/[module].rs
```

## 下一步

1. 选择要更新的模块
2. 阅读后端路由文件
3. 提取路由和处理器信息
4. 更新契约文档
5. 验证准确性
6. 提交更改

## 需要帮助？

如果遇到问题：
1. 查看 backend-route-inventory.md 了解路由总览
2. 查看 CHANGELOG.md 了解最近的变更
3. 查看后端代码注释
4. 运行后端测试了解预期行为
