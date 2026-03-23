# synapse-rust API 覆盖率分析 (v1.1)

> 基于 element-hq/synapse v1.149.1 对比

## 一、当前状态

### Client API 统计

| 类别 | synapse-rust | element-hq/synapse | 覆盖率 |
|------|-------------|-------------------|--------|
| **认证** | 34 | 35 | 97% |
| **房间** | 46 | 50 | 92% |
| **消息** | 38 | 40 | 95% |
| **媒体** | 18 | 20 | 90% |
| **用户** | 23 | 25 | 92% |
| **设备** | 17 | 18 | 94% |
| **同步** | 13 | 15 | 86% |
| **搜索** | 8 | 10 | 80% |

### Admin API 统计

| 类别 | synapse-rust | element-hq/synapse | 覆盖率 |
|------|-------------|-------------------|--------|
| **用户管理** | 25 | 27 | 93% |
| **房间管理** | 31 | 33 | 94% |
| **服务器** | 16 | 17 | 94% |
| **媒体** | 16 | 18 | 89% |
| **联邦** | 14 | 14 | 100% |
| **安全** | 10 | 10 | 100% |

## 二、新增 API 端点记录

### Client API
- `POST /_matrix/client/v3/keys/claim` - 声明一次性密钥
- `POST /_matrix/client/v3/keys/query` - 查询设备密钥
- `POST /_matrix/client/v3/account/3pid` - 管理 3PID (及相关子端点)
- `GET /_matrix/client/v3/joined_rooms` - 获取已加入的房间列表

### Admin API

#### 房间管理 (room.rs)
- `GET /_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}` - 获取事件上下文
- `POST /_synapse/admin/v1/rooms/{room_id}/search` - 搜索房间消息
- `GET /_synapse/admin/v1/rooms/{room_id}/forward_extremities` - 获取房间 forward extremities

#### 用户与认证 (token.rs)
- `GET /_synapse/admin/v1/registration_tokens` - 获取注册令牌
- `POST /_synapse/admin/v1/registration_tokens` - 创建注册令牌
- `GET /_synapse/admin/v1/registration_tokens/{token}` - 获取特定令牌
- `PUT /_synapse/admin/v1/registration_tokens/{token}` - 更新特定令牌
- `DELETE /_synapse/admin/v1/registration_tokens/{token}` - 删除特定令牌

#### 媒体管理 (media.rs)
- `GET /_synapse/admin/v1/media` - 获取所有媒体信息
- `GET /_synapse/admin/v1/media/{media_id}` - 获取特定媒体信息
- `DELETE /_synapse/admin/v1/media/{media_id}` - 删除特定媒体
- `GET /_synapse/admin/v1/media/quota` - 获取媒体配额
- `GET /_synapse/admin/v1/users/{user_id}/media` - 获取用户媒体
- `DELETE /_synapse/admin/v1/users/{user_id}/media` - 删除用户媒体
- `POST /_synapse/admin/v1/media/protect/{media_id}` - 保护媒体
- `POST /_synapse/admin/v1/media/unprotect/{media_id}` - 取消保护媒体

*(注：包括之前在 2026-03-19 添加的批处理、用户会话、强制加房等端点。)*

## 三、仍缺失的重要 API

### Client API

| 类别 | 缺失 API | 优先级 |
|------|----------|--------|
| **同步** | `GET /_matrix/client/v1/sync` (旧版) | 已补充 |

### Admin API

| 类别 | 缺失 API | 优先级 |
|------|----------|--------|
| **(无)** | (核心 API 已补齐) | - |

## 四、API 路由总数

```
Client API: ~237 路由
Admin API: ~174 路由
总计: ~411 路由
```

## 五、优化建议

### 短期 (1周)
1. 将 Admin 的各种 `stats` 接口（`room_stats`, `user_stats`, `statistics` 等）与运维仪表盘（Grafana/Prometheus等）对接

### 中期 (2-4周)
1. OIDC 完善
2. Push 通知优化
3. 激活并测试现有的 Worker 分布式架构 (`src/worker/`)

### 长期 (持续)
1. 测试覆盖提升
2. 文档完善
3. 性能优化

---

*创建日期: 2026-03-19*
*最后更新: 2026-03-22*
