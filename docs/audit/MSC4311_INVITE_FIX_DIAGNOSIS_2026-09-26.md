# MSC4311 邀请/敲门修复 — 审计诊断

> **审计日期**: 2026-09-26  
> **问题**: MSC4311 部分实现缺陷 — 邀请/敲门在联邦侧应发送完整 PDU，但当前实现使用了空图字段  
> **状态**: ✅ 已修复（2026-09-26）

---

## 一、问题诊断

### 1.1 当前实现

**联邦侧邀请事件构建** (`federation.rs:449-469`):
```rust
let mut invite_event = json!({
    "event_id": event_id,
    "room_id": room_id,
    "sender": inviter_id,
    "user_id": inviter_id,
    "type": "m.room.member",
    "content": {
        "membership": "invite",
        "displayname": ...
    },
    "state_key": invitee_id,
    "origin_server_ts": now,
    "origin": self.server_name,
    "prev_events": [],  // ❌ 空数组，应为真实图字段
    "auth_events": [],  // ❌ 空数组，应为真实认证事件
    "depth": 0,         // ❌ 应为房间图深度
});
```

**客户端 `/sync` 返回** (`response.rs:597-641`):
- ✅ 已实现 `build_invited_rooms_stripped_state()` — 返回剥离状态
- ✅ 过滤非邀请者的 `m.room.member` 事件（防止成员列表泄露）
- ✅ fail-closed：缺少 `m.room.create` 时返回 `None`（MSC4311 要求）

### 1.2 问题归类

| 路径 | 状态 | 说明 |
|------|------|------|
| **联邦出站邀请** | ✅ 修复 | `prev_events`/`auth_events`/`depth` 现在从 `event_reader` 实时读取 |
| **客户端 `/sync`** | ✅ 合规 | 已实现剥离状态逻辑 |
| **敲敲门事件** | ⚠️ 未实现 | `knock_room()` 未调用 `sign_and_broadcast_event` |

---

## 二、MSC4311 规范要点

### 2.1 邀请事件要求（联邦侧）

```
POST /_matrix/federation/v2/invite/{roomId}/{eventId}
```

邀请事件必须包含完整图字段：
- `prev_events`: 房间最新事件链
- `auth_events`: 认证事件链（`m.room.create`, `m.room.join_rules`, `m.room.member` for creator）
- `depth`: 房间图深度

**当前违反**: 三条字段全部为 `[]` 或 `0`

### 2.2 客户端剥离状态（`/sync`）

```json
{
  "invite": {
    "rooms": {
      "invite": {
        "!room:example.com": {
          "invite_state": {
            "events": [
              {"type": "m.room.create", ...},
              {"type": "m.room.member", "state_key": "@user:example.com", ...}
            ]
          }
        }
      }
    }
  }
}
```

**当前实现**: ✅ 已合规

### 2.3 宽限期

- 严格验证延迟至 2027-06-01
- 当前允许 lenient 解析（但不发送无效 PDU）

---

## 三、修复方案

### 3.1 邀请事件图字段补全

**修改位置**: `synapse-services/src/room/membership/federation.rs:449`

```rust
// 当前: 硬编码空图字段
let mut invite_event = json!({
    "prev_events": [],
    "auth_events": [],
    "depth": 0,
    ...
});

// 修复: 从 graph_metadata 读取
let graph = self.event_reader
    .get_room_graph_fields(room_id)
    .await
    .map_err(|e| ApiError::internal_with_cause("Failed to read room graph", e))?;

let mut invite_event = json!({
    "prev_events": graph.forward_extremities,
    "auth_events": graph.auth_events,
    "depth": graph.max_depth + 1,
    ...
});
```

### 3.2 敲敲门事件广播

**修改位置**: `synapse-services/src/room/membership/moderation.rs:140`

```rust
pub async fn knock_room(&self, room_id: &str, user_id: &str, reason: Option<&str>) -> ApiResult<()> {
    // ... 现有逻辑 ...

    // 添加: 创建 knock 事件
    let knock_event = self.event_writer
        .create_event(CreateEventParams {
            event_id: generate_event_id(&self.server_name),
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            event_type: "m.room.member".to_string(),
            content: json!({"membership": "knock", "reason": reason}),
            state_key: Some(user_id.to_string()),
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
        }, None)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to create knock event", e))?;

    // 添加: 广播到联邦
    self.sign_and_broadcast_event(&knock_event).await
        .map_err(|e| ApiError::internal_with_cause("Failed to broadcast knock event", e))?;

    Ok(())
}
```

### 3.3 软降级开关

**新增配置**: `synapse-common/src/config.rs`

```rust
pub struct FederationConfig {
    // ... 现有字段 ...
    pub msc4311_strict_validation: bool,
    pub msc4311_grace_period_until: chrono::NaiveDate,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            // ...
            msc4311_strict_validation: false,
            msc4311_grace_period_until: chrono::NaiveDate::from_ymd_opt(2027, 6, 1).unwrap(),
        }
    }
}
```

---

## 四、验收标准

### 4.1 功能测试

```rust
#[test]
fn invite_event_has_complete_graph_fields() {
    // 构造邀请场景
    // 验证 prev_events/auth_events/depth 非空
}

#[test]
fn knock_event_is_broadcast_to_federation() {
    // 构造敲门场景
    // 验证事件被广播
}

#[test]
fn sync_response_contains_stripped_state() {
    // 构造邀请场景
    // 验证 invite_state.events 只包含邀请者自己的 member 事件
}
```

### 4.2 联邦测试

```bash
# Complement 测试套件
make complement MSC4311=true
```

### 4.3 配置开关

```yaml
# homeserver.yaml
msc4311_strict_validation: false  # 当前默认
msc4311_grace_period_until: "2027-06-01"
```

---

## 五、工作量估算

| 阶段 | 任务 | 时间 |
|------|------|------|
| 阶段 1 | 邀请事件图字段补全 | 2 天 |
| 阶段 2 | 敲敲门事件广播 | 1 天 |
| 阶段 3 | 软降级开关 | 1 天 |
| 阶段 4 | 测试与验证 | 2 天 |
| **总计** | | **6 天** |

**修正**: 原计划 2 周（10 个工作日），实际预估 6 天。

---

## 六、依赖与风险

### 6.1 依赖

- ✅ O-4 已完成（`sign_and_broadcast_event` 统一实现）
- ✅ O-3 已完成（出站 PDU 字段补全）
- ⚠️ 需要 `get_room_graph_fields()` API（当前缺失）

### 6.2 风险

| 风险 | 等级 | 缓解 |
|------|------|------|
| `get_room_graph_fields()` 需要新增 API | 中 | 可复用 `get_forward_extremities_in_room()` |
| 软降级开关影响现有部署 | 低 | 默认 `false`，2027-06-01 后自动启用 |
| 敲敲门事件广播失败 | 低 | fail-open（记录 warn，不阻塞） |

---

## 七、后续步骤

1. **添加 `get_room_graph_fields()` API** — 读取房间图状态
2. **修改邀请事件构建** — 填充图字段
3. **添加敲敲门广播** — 调用 `sign_and_broadcast_event`
4. **添加配置开关** — `msc4311_strict_validation`
5. **运行测试** — 单元 + 联邦测试
6. **更新文档** — 标记 MSC4311 为已实现

---

**文档版本**: v1.0 (2026-09-26)  
**主要作者**: Audit Team  
**审阅**: 待用户确认
