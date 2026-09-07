# E-07: 密钥轮换服务 forward_keys_for_new_member 缺少重放保护与速率限制

## 严重等级

🟡 **Medium** — 新成员加入时可被滥用放大 to-device 流量

## 涉及代码

- `synapse-e2ee/src/key_rotation/service.rs:forward_keys_for_new_member`（行 ~150–200）
- `synapse-e2ee/src/key_rotation/service.rs:share_session`（megolm）
- 涉及表：`megolm_key_shares`

## 问题描述

`KeyRotationService.forward_keys_for_new_member` 在新成员加入加密房间时，通过 to-device 端点转发 Megolm 会话密钥给该成员：

```rust
// key_rotation/service.rs
pub async fn forward_keys_for_new_member(
    &self,
    user_id: &UserId,
    device_id: &str,
    room_id: &str,
) -> ApiResult<()> {
    // 1. 查找房间当前 outbound Megolm session
    let outbound = self.megolm_service
        .get_outbound_session(room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No active Megolm session"))?;

    // 2. 分享 session
    self.megolm_service
        .share_session(room_id, &outbound.session_id, user_id, device_id)
        .await?;

    // 3. 记录轮换日志
    self.storage.log_key_rotation(...).await;
    Ok(())
}
```

### 风险 A：无去重检查

- `megolm_key_shares` 表记录了"已分享"的 (session_id, recipient_user_id, recipient_device_id) 三元组。
- 但 `forward_keys_for_new_member` **在记录日志前就发送了 to-device**，即没有查询"是否已分享过"。
- 同一 (user, device, room) 短时间内多次调用 → 多次 to-device `m.room_key` 消息发送。

### 风险 B：无速率限制

- to-device 端点 `/sendToDevice` 本身没有针对 `m.room_key` 类型的专项限流。
- 攻击者可调用 `/sync?filter=...` 反复触发同步，同步中隐式触发 `forward_keys_for_new_member`（如果成员列表变化）。

### 风险 C：与 E-04（无重放保护）联动

- 若攻击者截获 to-device `m.room_key` 消息，可结合 E-04 的"无 message_index 重放检测"：攻击者将 `m.room_key` 重放给同一设备，设备会接受并更新 inbound session state（vodozemac 接受更新的 ratchet state）。

## 修复建议

```rust
// key_rotation/service.rs
pub async fn forward_keys_for_new_member(...) -> ApiResult<()> {
    let key = (session_id.clone(), user_id.to_string(), device_id.to_string());

    // ✓ 查询是否已分享
    if self.storage.has_shared_key(&key).await? {
        tracing::debug!("Key already shared to {}/{} in {}. Skipping.", user_id, device_id, room_id);
        return Ok(());
    }

    // 分享 session
    self.megolm_service.share_session(...).await?;

    // ✓ 记录日志
    self.storage.log_key_share(&key).await?;

    return Ok(());
}
```

同时在 `megolm_key_shares` 表加唯一约束：
```sql
ALTER TABLE megolm_key_shares
ADD CONSTRAINT megolm_key_shares_pkey
PRIMARY KEY (session_id, recipient_user_id, recipient_device_id);
```

## 测试覆盖

现有测试 `test_forward_keys_for_new_member` 覆盖正常路径。未覆盖：
- 重复调用 → 期望只发送一次 to-device。
- 恶意用户反复加入/离开 → 期望 rate-limit。

## 与 E-04 的关联

若 E-04（Megolm 无 message_index 重放检测）被利用，则攻击者收到 `m.room_key` 后可将相同 session_key 重放给自己，实现**会话绑架**（session hijacking）。
