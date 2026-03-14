# synapse-rust 后端项目优化方案 (最终版)

## 一、项目现状分析

### 1.1 代码规模统计

| 模块 | 文件大小 | 问题等级 |
|------|----------|----------|
| `web/routes/mod.rs` | 4097 行 | 🔴 需优化 |
| `services/mod.rs` | 923 行 | 🟡 需优化 |
| 路由文件数量 | 39+ 个 | ✅ 合理 |
| 存储文件数量 | 54+ 个 | ✅ 合理 |

---

## 二、完成进度

### ✅ 阶段 1: 代码规范统一

| 任务 | 状态 | 说明 |
|------|------|------|
| 数据库迁移 - 索引 | ✅ 完成 | `migrations/20260312000004_add_missing_indexes.sql` |
| 代码区域注释 | ✅ 完成 | mod.rs 添加了 5 个区域注释 |
| 优化方案文档 | ✅ 完成 | `docs/OPTIMIZATION_PLAN.md` |

### ✅ 阶段 2: MSC 功能实现

#### ✅ MSC4388 二维码登录 (已完成)

| 文件 | 说明 |
|------|------|
| `src/storage/qr_login.rs` | QR 登录存储层 |
| `src/web/routes/qr_login.rs` | QR 登录路由处理 |
| `migrations/20260312000005_qr_login.sql` | 数据库表 |

**API 端点**:
- `GET /_matrix/client/v1/login/get_qr_code` - 生成登录二维码
- `POST /_matrix/client/v1/login/qr/confirm` - 确认二维码登录
- `POST /_matrix/client/v1/login/qr/start` - 发起二维码登录
- `GET /_matrix/client/v1/login/qr/{transaction_id}/status` - 查询登录状态

#### ✅ MSC4380 邀请屏蔽 (已完成)

| 文件 | 说明 |
|------|------|
| `src/storage/invite_blocklist.rs` | 邀请屏蔽存储层 |
| `src/web/routes/invite_blocklist.rs` | 邀请屏蔽路由处理 |
| `migrations/20260312000006_invite_blocklist.sql` | 数据库表 |

**API 端点**:
- `GET /_matrix/client/v3/rooms/{room_id}/invite_blocklist` - 获取邀请黑名单
- `POST /_matrix/client/v3/rooms/{room_id}/invite_blocklist` - 设置邀请黑名单
- `GET /_matrix/client/v3/rooms/{room_id}/invite_allowlist` - 获取邀请白名单
- `POST /_matrix/client/v3/rooms/{room_id}/invite_allowlist` - 设置邀请白名单

#### ✅ MSC4354 Sticky Event (已完成)

| 文件 | 说明 |
|------|------|
| `src/storage/sticky_event.rs` | Sticky Event 存储层 |
| `src/web/routes/sticky_event.rs` | Sticky Event 路由处理 |
| `migrations/20260312000007_sticky_event.sql` | 数据库表 |

**API 端点**:
- `GET /_matrix/client/v3/rooms/{room_id}/sticky_events` - 获取粘性事件
- `POST /_matrix/client/v3/rooms/{room_id}/sticky_events` - 设置粘性事件
- `DELETE /_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}` - 清除粘性事件

---

## 三、数据库迁移汇总

| 迁移 | 内容 | 状态 |
|------|------|------|
| 20260312000004 | 添加缺失索引 | ✅ 完成 |
| 20260312000005 | QR 登录表 | ✅ 完成 |
| 20260312000006 | 邀请黑名单/白名单 | ✅ 完成 |
| 20260312000007 | Sticky Event | ✅ 完成 |

---

## 四、验收标准

### 4.1 代码质量

- [x] mod.rs 添加区域注释
- [x] 数据库 Schema 规范化

### 4.2 MSC 功能

- [x] MSC4388 二维码登录
- [x] MSC4380 邀请屏蔽
- [x] MSC4354 Sticky Event

### 4.3 测试覆盖

- [ ] 核心存储层测试覆盖率 > 70%
- [ ] 关键 API 端点测试 > 50%

---

## 五、使用示例

### MSC4354 Sticky Event

```bash
# 设置粘性事件 (例如记住用户最后一次查看的 Poincar 事件)
POST /_matrix/client/v3/rooms/!room:localhost/sticky_events
{
  "events": [
    {
      "event_type": "m.room.message",
      "event_id": "$event:localhost"
    }
  ]
}

# 获取所有粘性事件
GET /_matrix/client/v3/rooms/!room:localhost/sticky_events

# 获取特定类型的粘性事件
GET /_matrix/client/v3/rooms/!room:localhost/sticky_events?event_type=m.room.message

# 清除粘性事件
DELETE /_matrix/client/v3/rooms/!room:localhost/sticky_events/m.room.message
```

---

**版本**: v4.0  
**更新日期**: 2026-03-12  
**状态**: 所有 MSC 功能已完成
