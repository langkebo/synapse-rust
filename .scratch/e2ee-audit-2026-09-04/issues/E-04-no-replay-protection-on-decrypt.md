# E-04: Olm/Megolm 解密路径缺少服务器侧消息重放保护

## 严重等级

🟡 **Medium** — Olm/Megolm 自身 ratchet 提供基础保护，但服务器层缺少主动校验

## 涉及代码

- `synapse-e2ee/src/olm/session.rs:decrypt`（行 ~140）
- `synapse-e2ee/src/vodozemac_megolm.rs:decrypt`（行 ~110）

## 问题描述

Olm/Megolm 的 ratchet 机制本身对消息重放有天然抵抗力（每个消息索引只能被正确解密一次），但服务器层代码**没有显式校验** `message_index` 是否存在跳跃或重复。

### Olm decrypt

```rust
// synapse-e2ee/src/olm/session.rs:decrypt
pub async fn decrypt(&self, message: &[u8]) -> ApiResult<OlmDecryptedPayload> {
    // 1. 查找会话
    let entry = self.get_session_for_sender(sender_key).await?
        .ok_or_else(|| ApiError::not_found("No Olm session found"))?;

    // 2. 解密
    let message = OlmMessage::from_parts(message)?;
    let plaintext = entry.session.decrypt(&message)
        .map_err(|e| ApiError::internal_error(e.to_string()))?;

    // 3. ⚠ 无 message_index 重复/跳跃校验
    self.persist_session(&entry).await;

    Ok(OlmDecryptedPayload { plaintext, sender_key, session_id })
}
```

### Megolm decrypt

```rust
// synapse-e2ee/src/vodozemac_megolm.rs:decrypt
pub async fn decrypt(
    &self,
    sender_key: &str,
    session_id: &str,
    message: &[u8],
) -> ApiResult<MegolmDecryptedPayload> {
    let inbound = self.get_inbound_group_session(sender_key, session_id).await?
        .ok_or_else(|| ApiError::not_found("Inbound group session not found"))?;

    let megolm_message = MegolmMessage::from_bytes(message)
        .map_err(|e| ApiError::internal_error(e.to_string()))?;

    // ⚠ 仅有 vodozemac 内部的 ratchet 保护，无显式 message_index 校验
    let decrypted = inbound.decrypt(&megolm_message)
        .map_err(|e| ApiError::internal_error(e.to_string()))?;

    self.persist_best_effort(&inbound).await;

    Ok(MegolmDecryptedPayload { plaintext: decrypted.plaintext, message_index: decrypted.message_index })
}
```

## Olm/Megolm ratchet 的实际保护能力

- **Olm**：`Session::decrypt` 在 ratchet 中维护当前 ratchet 状态，解密后会推进。**但**：vodozemac 的 `decrypt` 默认对旧消息**也允许解密**（会回退 ratchet），即非前向拒绝（not strictly forward-secure）。攻击者可重放旧消息，服务器不会拒绝。
- **Megolm**：`InboundGroupSession::decrypt` 接受指定 `message_index` 的消息，**不会**检查 message_index 是否已使用。也就是说，攻击者可以**重放同一 message_index 的密文**到 to-device 端点，服务器会重复解密并触发后续副作用（如果上层业务有副作用逻辑）。

## 利用场景（Exploit Scenario）

### 场景 A：to-device 消息重放

1. 攻击者截获受害者 A 发送给 B 的 to-device 消息（密文 + session_id + sender_key）。
2. 攻击者以自己的会话再次 PUT `/_matrix/client/v3/sendToDevice/{type}/{txnId}`（伪造事务 ID），重放相同密文。
3. 服务器 Megolm decrypt 成功（vodozemac 允许重放），B 收到重复处理。
4. 若业务层（如 key 转发、签名验证）无幂等保护，B 会重复执行副作用。

### 场景 B：ratchet 状态回退攻击

1. 攻击者通过侧信道获取 ratchet 状态 dump（例如服务器 pickle 泄漏）。
2. 攻击者将服务器 ratchet 重置到旧 state。
3. 旧密文再次可解密，绕过 ratchet 推进。

服务器层无显式 message_index 跟踪，无法检测此攻击。

## 修复建议

### 短期：在 decrypt 后校验 message_index 单调性

```rust
// OlmSessionManager.decrypt
let last_index = entry.last_decrypted_index;
let new_index = decrypted_message_index;
if let Some(prev) = last_index {
    if new_index <= prev {
        return Err(ApiError::forbidden("Olm message index regressed (potential replay)"));
    }
}
entry.last_decrypted_index = Some(new_index);
```

### 中期：服务端维护已见 message_index 集合

类似 Synapse Python 实现：
```rust
// MegolmVodozemacService.decrypt
let seen = storage.get_seen_message_indexes(session_id).await?;
if seen.contains(&new_index) {
    return Err(ApiError::forbidden("Megolm message index already seen (replay)"));
}
storage.record_seen_message_index(session_id, new_index).await?;
```

### 长期：依赖 Matrix 规范层面规范

Matrix MSC3967 引入了 `m.replay_detection` 字段，期望客户端在事件中携带 `m.relation` 防止 to-device 重放。服务器层可作为补充检测层。

## 与其他 Issue 的关联

- **E-07（密钥轮换）**：`key_rotation_service.forward_keys_for_new_member` 在新成员加入时重发会话密钥，但若 to-device 端点无重放保护，重发可被攻击者捕获并离线重放。
- **E-06（pickle key 持久化）**：pickle 泄漏会暴露 ratchet state，使重放检测绕过。

## 风险评级说明

- vodozemac 0.9 的 ratchet 本身可正确处理**前向**消息（每个 message_index 唯一解密结果），但**后向**（重放）由上层负责。
- 服务器层不跟踪 message_index 是行业惯例（依赖客户端 idempotency），但服务端是攻击者可主动控制的层，至少应有告警 + 限速机制。
