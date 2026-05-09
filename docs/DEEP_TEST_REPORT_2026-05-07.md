# 深度联调测试问题清单（第二轮）

**测试日期**: 2026-05-07
**后端地址**: https://matrix.test
**测试范围**: 之前未覆盖的深度功能测试
**测试端点数**: 35+

---

## 一、严重问题 (Critical / High)

### NEW-01: 非成员可访问私有房间消息 [Critical - 安全漏洞]
- **现象**: 用户不是某私有房间成员，但仍可通过 `/messages` API 获取该房间的全部消息
- **复现步骤**:
  1. 用户A创建私有房间 `!6GT-DeS1wKyQTvPqx84PdENr:matrix.test`
  2. 用户B（非成员）调用 `GET /rooms/!6GT-.../messages?dir=b&limit=5`
  3. 返回 200 和房间的全部状态事件（7条），包括 power_levels、member 等
- **实际返回**: `{"chunk":[...7个事件...],"end":"t1778131542699","start":"0"}`
- **期望行为**: 返回 403 Forbidden，非成员不应能访问私有房间消息
- **影响范围**: 所有私有房间的消息安全性，用户隐私泄露
- **严重程度**: Critical（安全漏洞）
- **优化方案**: 在 `/messages` handler 中增加成员权限检查，非成员访问私有房间时返回 403
- **优先级**: P0 - 立即修复

### NEW-02: /messages API 忽略 limit 参数 [High]
- **现象**: `GET /rooms/{roomId}/messages?dir=b&limit=3` 返回超过 3 条事件
- **复现步骤**:
  1. 调用 `GET /rooms/{roomId}/messages?dir=b&limit=3`
  2. 观察返回的 chunk 包含远超 3 条事件
- **影响范围**: 消息分页功能异常，客户端无法正确分页加载历史消息
- **严重程度**: High
- **优化方案**: 修改 `/messages` handler 的查询逻辑，正确应用 limit 参数到 SQL 查询
- **优先级**: P0

### NEW-03: /messages API 缺少 start/end pagination token [High]
- **现象**: `/messages` 返回的 `start` 值为 `"0"`，`end` 值为 `"t1778131542699"`，start token 格式异常
- **期望行为**: `start` 和 `end` 都应为有效的 pagination token（如 `t1778131542700` 格式）
- **影响范围**: 客户端无法正确进行消息分页，无法加载更早的历史消息
- **严重程度**: High
- **优化方案**: 修正 start token 的生成逻辑，使用正确的 stream_ordering 或 timestamp token
- **优先级**: P1

### NEW-04: 同步中不返回 ephemeral 事件（Receipt/Typing） [High]
- **现象**: 即使已发送 receipt 和 typing 通知，同步响应中 `ephemeral` 事件列表始终为空
- **复现步骤**:
  1. 发送 receipt: `POST /rooms/{roomId}/receipt/m.read/{eventId}`
  2. 设置 typing: `PUT /rooms/{roomId}/typing/{userId}`
  3. 执行同步
  4. 观察房间 join 数据中 `ephemeral.events` 为空数组
- **影响范围**: 客户端无法显示已读回执和输入状态，严重影响用户体验
- **严重程度**: High
- **优化方案**: 修改 sync_service 的房间数据组装逻辑，查询并填充 ephemeral 事件（m.receipt、m.typing）
- **优先级**: P1

### NEW-05: 公开房间列表始终为空 [High]
- **现象**: `GET /publicRooms` 返回 `{"chunk":[],"total_room_count_estimate":0}`
- **复现步骤**:
  1. 创建房间并设置 `join_rule: "public"`
  2. 调用 `GET /publicRooms`
  3. 返回空列表
- **影响范围**: 用户无法通过公开房间目录发现和加入房间
- **严重程度**: High
- **优化方案**: 修改房间创建/状态变更逻辑，当 join_rule 设为 public 时，将房间添加到公开目录；修改 publicRooms 查询从目录表读取
- **优先级**: P1

---

## 二、中等问题 (Medium)

### NEW-06: Leave Room 返回内部字段 [Medium]
- **现象**: `POST /rooms/{roomId}/leave` 返回 `{"left_ts":1778133448870,"room_id":"!fwccRIeRpNBXmSQRciT890tn:matrix.test"}`
- **期望返回**: `{}`（空对象，Matrix 规范）
- **影响范围**: 前端可能解析异常
- **严重程度**: Medium
- **优化方案**: 修改 leave handler 返回空对象
- **优先级**: P2

### NEW-07: 媒体下载 Content-Disposition 缺少文件名 [Medium]
- **现象**: 媒体下载响应头 `Content-Disposition: attachment`，缺少 `filename` 参数
- **期望行为**: `Content-Disposition: inline; filename="test.txt"` 或 `attachment; filename="test.txt"`
- **影响范围**: 浏览器下载时无法显示正确文件名
- **严重程度**: Medium
- **优化方案**: 修改媒体下载 handler，从数据库读取 filename 并设置到 Content-Disposition
- **优先级**: P2

### NEW-08: 媒体下载 Content-Type 始终为 application/octet-stream [Medium]
- **现象**: 即使上传时指定了 `content_type=text/plain`，下载时返回 `content-type: application/octet-stream`
- **影响范围**: 浏览器无法正确预览媒体内容（如图片、PDF等）
- **严重程度**: Medium
- **优化方案**: 修改媒体下载 handler，从数据库读取 content_type 并设置到响应头
- **优先级**: P2

### NEW-09: URL Preview API 返回 404 [Medium]
- **现象**: `GET /_matrix/client/v1/media/preview_url?url=https://example.com` 返回 404
- **影响范围**: 客户端无法显示链接预览卡片
- **严重程度**: Medium
- **优化方案**: 实现 URL Preview 端点，或返回 501 Not Implemented 而非 404
- **优先级**: P2

### NEW-10: VoIP 信令事件反序列化错误 [Medium]
- **现象**: 发送 `m.call.invite` 事件时返回 422: `invitee: invalid type: map, expected a string`
- **根因**: `m.call.invite` 的 `invitee` 字段在 Matrix VoIP v1 中是字符串（用户ID），但 Element 使用的是 VoIP v1 格式
- **影响范围**: VoIP 通话邀请无法发送
- **严重程度**: Medium
- **优化方案**: 检查 VoIP 事件的反序列化逻辑，`invitee` 应接受字符串类型
- **优先级**: P2

### NEW-11: 密钥备份创建要求 auth_data.signatures [Medium]
- **现象**: `POST /room_keys/version` 不带 signatures 时返回 400: `auth_data must contain signatures`
- **分析**: Synapse 原版不强制要求 signatures 字段，允许用户先创建备份再验证
- **影响范围**: 某些客户端可能无法创建密钥备份
- **严重程度**: Medium
- **优化方案**: 放宽验证，signatures 字段为可选
- **优先级**: P2

### NEW-12: 搜索结果包含非消息事件 [Medium]
- **现象**: 搜索 "test" 时返回了 `m.room.member`、`m.room.redaction` 等非消息事件
- **期望行为**: 默认搜索应仅返回 `m.room.message` 类型事件
- **影响范围**: 搜索结果包含无关事件，降低搜索精度
- **严重程度**: Medium
- **优化方案**: 修改搜索逻辑，默认过滤 event_type = 'm.room.message'
- **优先级**: P2

---

## 三、低优先级问题 (Low)

### NEW-13: TURN Server 配置为空 [Low]
- **现象**: `GET /voip/turnServer` 返回空配置 `{"username":"","password":"","uris":[],"ttl":0}`
- **影响范围**: VoIP 通话无法使用 TURN 中继
- **严重程度**: Low（需配置 TURN 服务器）
- **优先级**: P3

### NEW-14: Dehydrated Device API 返回 404 [Low]
- **现象**: `GET /unstable/org.matrix.msc3814.v1/dehydrated_device` 返回 404
- **分析**: 这是 MSC3814 实验性功能，返回 404 表示未实现，属正常行为
- **严重程度**: Low
- **优先级**: P3

---

## 四、测试结果总览

| API 类别 | 测试端点数 | 通过 | 部分通过 | 失败 | 新发现问题 |
|---------|----------|------|---------|------|----------|
| 房间状态变更 | 3 | 3 | 0 | 0 | 0 |
| 成员状态转换 | 5 | 5 | 0 | 0 | 0 |
| 消息分页 | 1 | 0 | 0 | 1 | 3 |
| 用户Profile | 3 | 2 | 1 | 0 | 0 |
| 房间别名 | 2 | 2 | 0 | 0 | 0 |
| 公开房间目录 | 1 | 0 | 0 | 1 | 1 |
| Receipt/Typing | 2 | 2 | 0 | 0 | 1 |
| Filter API | 2 | 2 | 0 | 0 | 0 |
| 密钥备份 | 4 | 3 | 0 | 1 | 1 |
| Redaction | 1 | 1 | 0 | 0 | 0 |
| 错误处理 | 1 | 1 | 0 | 0 | 0 |
| 媒体下载 | 2 | 0 | 1 | 1 | 2 |
| Room Account Data | 2 | 2 | 0 | 0 | 0 |
| URL Preview | 1 | 0 | 0 | 1 | 1 |
| VoIP | 1 | 0 | 0 | 1 | 1 |
| 搜索 | 1 | 0 | 1 | 0 | 1 |
| Leave Room | 1 | 0 | 1 | 0 | 1 |
| **总计** | **33** | **23** | **4** | **6** | **14** |

---

## 五、优先级排序

### P0 - 立即修复（安全/核心功能）
1. **NEW-01**: 非成员可访问私有房间消息 → 增加 /messages 权限检查
2. **NEW-02**: /messages 忽略 limit 参数 → 修正 SQL 查询

### P1 - 尽快修复（重要功能）
3. **NEW-03**: /messages 缺少正确 pagination token → 修正 token 生成
4. **NEW-04**: 同步不返回 ephemeral 事件 → 填充 receipt/typing 数据
5. **NEW-05**: 公开房间列表始终为空 → 实现公开目录逻辑

### P2 - 计划修复（体验/兼容性）
6. **NEW-06**: Leave Room 返回内部字段 → 返回空对象
7. **NEW-07**: 媒体下载缺少文件名 → 设置 Content-Disposition filename
8. **NEW-08**: 媒体下载 Content-Type 错误 → 从数据库读取
9. **NEW-09**: URL Preview 404 → 实现或返回 501
10. **NEW-10**: VoIP 事件反序列化错误 → 修正 invitee 类型
11. **NEW-11**: 密钥备份签名强制 → 放宽验证
12. **NEW-12**: 搜索包含非消息事件 → 默认过滤

### P3 - 后续优化
13. **NEW-13**: TURN Server 配置为空 → 需运维配置
14. **NEW-14**: Dehydrated Device 404 → MSC3814 未实现

---

## 六、技术实现建议

### NEW-01 修复方案（安全漏洞）

在 `handlers/room.rs` 的 `get_room_messages` handler 中增加权限检查：

```rust
// 在返回消息前检查用户是否是房间成员
let is_member = state.services.member_storage
    .is_member(&room_id, &user_id).await?;
let room = state.services.room_storage.get_room(&room_id).await?;
let is_public = room.map(|r| r.is_public).unwrap_or(false);

if !is_public && !is_member {
    return Err(ApiError::forbidden(
        "You are not a member of this room".to_string(),
    ));
}
```

### NEW-04 修复方案（ephemeral 事件）

在 `sync_service.rs` 的房间数据组装中，查询 receipt 和 typing 数据：

```rust
// 在 build_room_sync_data 中添加
let ephemeral_events = self.get_ephemeral_events(room_id, since_token).await?;
if !ephemeral_events.is_empty() {
    room_data.ephemeral = json!({ "events": ephemeral_events });
}
```
