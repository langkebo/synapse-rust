# synapse-rust API 覆盖率分析

> 基于 element-hq/synapse v1.149.1 对比

## 一、当前状态

### Client API 统计

| 类别 | synapse-rust | element-hq/synapse | 覆盖率 |
|------|-------------|-------------------|--------|
| **认证** | 34 | 35 | 97% |
| **房间** | 45 | 50 | 90% |
| **消息** | 38 | 40 | 95% |
| **媒体** | 18 | 20 | 90% |
| **用户** | 22 | 25 | 88% |
| **设备** | 15 | 18 | 83% |
| **同步** | 12 | 15 | 80% |
| **搜索** | 8 | 10 | 80% |

### Admin API 统计

| 类别 | synapse-rust | element-hq/synapse | 覆盖率 |
|------|-------------|-------------------|--------|
| **用户管理** | 20 | 22 | 91% |
| **房间管理** | 28 | 30 | 93% |
| **服务器** | 15 | 17 | 88% |
| **媒体** | 8 | 10 | 80% |
| **联邦** | 10 | 12 | 83% |
| **安全** | 8 | 10 | 80% |

## 二、新增 Admin API 端点 (2026-03-19 08:30)

### 已添加的端点

#### 用户管理 (user.rs)
- `POST /_synapse/admin/v1/users/batch` - 批量创建用户
- `POST /_synapse/admin/v1/users/batch_deactivate` - 批量停用用户
- `GET /_synapse/admin/v1/user_sessions/{user_id}` - 获取用户会话
- `POST /_synapse/admin/v1/user_sessions/{user_id}/invalidate` - 使会话失效
- `GET /_synapse/admin/v1/account/{user_id}` - 获取账户详情
- `POST /_synapse/admin/v1/account/{user_id}` - 更新账户

#### 房间管理 (room.rs)
- `PUT /_synapse/admin/v1/rooms/{room_id}/members/{user_id}` - 强制加入房间
- `DELETE /_synapse/admin/v1/rooms/{room_id}/members/{user_id}` - 移除成员
- `POST /_synapse/admin/v1/rooms/{room_id}/ban/{user_id}` - 封禁用户
- `POST /_synapse/admin/v1/rooms/{room_id}/unban/{user_id}` - 解封用户
- `POST /_synapse/admin/v1/rooms/{room_id}/kick/{user_id}` - 踢出用户
- `GET /_synapse/admin/v1/rooms/{room_id}/listings` - 获取房间列表状态
- `PUT /_synapse/admin/v1/rooms/{room_id}/listings/public` - 设为公开
- `DELETE /_synapse/admin/v1/rooms/{room_id}/listings/public` - 设为私有

#### 统计 (room.rs)
- `GET /_synapse/admin/v1/room_stats` - 全局房间统计
- `GET /_synapse/admin/v1/room_stats/{room_id}` - 单房间统计

## 三、仍缺失的重要 API

### Client API

| 类别 | 缺失 API | 优先级 |
|------|----------|--------|
| **设备** | `POST /_matrix/client/v3/keys/claim` (一次性密钥) | 高 |
| **设备** | `POST /_matrix/client/v3/keys/query` | 高 |
| **同步** | `GET /_matrix/client/v1/sync` (旧版) | 中 |
| **用户** | `POST /_matrix/client/v3/account/3pid` | 中 |
| **房间** | `GET /_matrix/client/v3/joined_rooms` | 高 |

### Admin API

| 类别 | 缺失 API | 优先级 |
|------|----------|--------|
| **房间** | `POST /_synapse/admin/v1/purge_room` | 高 |
| **房间** | `GET /_synapse/admin/v1/rooms/{room_id}/event_context` | 中 |
| **用户** | `GET /_synapse/admin/v1/registration_tokens` | 中 |
| **媒体** | `GET /_synapse/admin/v1/media/{server_name}` | 中 |
| **服务器** | `GET /_synapse/admin/v1/backups` | 低 |

## 四、API 路由总数

```
Client API: 232 路由
Admin API: 157 路由
总计: 389 路由
```

## 五、优化建议

### 短期 (1周)
1. 补充缺失的高优先级设备 API
2. 补充房间管理 API
3. 完善 Admin 统计功能

### 中期 (2-4周)
1. OIDC 完善
2. Push 通知优化
3. Worker 架构设计

### 长期 (持续)
1. 测试覆盖提升
2. 文档完善
3. 性能优化

---

*创建日期: 2026-03-19*
*最后更新: 2026-03-19 08:35*
