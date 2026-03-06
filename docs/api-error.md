# synapse-rust API 问题报告

> 生成时间: 2026-03-06

## 测试环境
- 后端: synapse-rust (本地编译)
- 测试用户: testuser / TestPass123!
- 服务器: http://localhost:8008

---

## API 测试结果

### ✅ 正常 API (13个)
| API | 状态 |
|-----|------|
| /versions | ✅ |
| /login | ✅ |
| /r0/joined_rooms | ✅ |
| /r0/createRoom | ✅ |
| /r0/join/{roomId} | ✅ |
| /r0/leave | ✅ |
| /r0/rooms/{id}/members | ✅ |
| /r0/rooms/{id}/messages | ✅ |
| /r3/user_directory/search | ✅ |
| /r0/account/whoami | ✅ |
| /r0/profile/{userId}/displayname | ✅ |
| /r0/profile/{userId}/avatar_url | ✅ |
| /r0/register | ✅ |

### ❌ 有问题 API (6个)

| # | API | 问题 | 状态 |
|---|-----|------|------|
| 1 | /v3/capabilities | 路由返回404 | 🔧 调试中 |
| 2 | /v3/pushrules/ | 路由返回空 | 🔧 调试中 |
| 3 | /r0/rooms/{id}/initialSync | 返回空 | 🔧 已添加代码 |
| 4 | /r0/rooms/{id}/send/m.room.message | 返回空 | 🔧 调试中 |
| 5 | /r0/rooms/{id}/event/{eventId} | 返回空 | 🔧 调试中 |
| 6 | /profile/{userId} (GET) | M_UNAUTHORIZED | 🔧 调试中 |

---

## 已知问题

### v3 路由问题
- r0 版本工作正常，v3 版本返回 404
- 路由代码已添加但 Axum 匹配失败
- 可能是 merge 顺序或路由覆盖问题

### initialSync
- 代码已添加但返回空响应
- 可能被其他路由拦截

### 消息发送
- sendMessage 返回空响应
- 事件存储逻辑需要调试

---

## 已提交更改
- 添加 initialSync 处理函数
- push.rs 添加尾随斜杠支持
- mod.rs 添加内联 pushrules

