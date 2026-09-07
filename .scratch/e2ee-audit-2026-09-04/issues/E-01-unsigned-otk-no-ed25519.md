# E-01: signed_curve25519 一次性密钥在无 ed25519 设备密钥时绕过签名校验

## 严重等级

🟡 **Medium-High** — 可导致中间人攻击（MITM）下嫁注伪造的一次性密钥

## 涉及代码

- `synapse-e2ee/src/device_keys/service.rs:320–331`（普通 signed_curve25519 OTK）
- `synapse-e2ee/src/device_keys/service.rs:415–428`（fallback_keys 中的 signed_curve25519）

## 问题描述

`DeviceKeyService.upload_keys` 对两类 signed_curve25519 一次性密钥做签名校验时存在两个未验证分支：

### 分支 A：普通 signed_curve25519（行 320–331）

```rust
if let Some(device_key) = device_key_opt {
    if let Some(device_ed25519_key) = device_key.keys.get("ed25519") {
        // ✓ 校验签名
        Self::verify_one_time_key_signature(...)?;
    } else {
        // ⚠ 仅 warn，未拒绝，也未校验
        tracing::warn!(
            "No ed25519 device key found for user {} device {}; \
             storing OTK without signature verification",
            user_id, device_id
        );
        // fall through → 仍然存储
    }
}
```

### 分支 B：fallback_keys（行 415–428）

```rust
if let Some(ed25519_key) = device_ed25519_key {
    // ✓ 校验签名
    Self::verify_one_time_key_signature(...)?;
} else {
    // ⚠ 同上，仅 warn
    tracing::warn!("No ed25519 device key found for user {} device {}; \
                    storing fallback OTK without signature verification", ...);
}
```

对比：当 ed25519 设备密钥存在时，签名校验失败 → `return Err(ApiError::bad_request(...))`（行 ~302），与 E-05/F-06 保持一致。但无 ed25519 密钥时，校验被静默跳过。

## 利用场景（Exploit Scenario）

1. 攻击者伪造一个不含 `ed25519` 密钥的设备（只有 `signed_curve25519`），上传到目标用户的设备列表中。
2. 受害者的 SDK 从该设备请求一次性密钥（`claim_keys`）。
3. 服务器返回攻击者构造的公钥（攻击者持有对应私钥）。
4. 受害者用此公钥建立 Olm 会话 → 攻击者可解密后续所有 1:1 消息。

关键前提：攻击者需要能以受害者设备身份上传"没有 ed25519 密钥"的设备声明，这在实际 Matrix 协议中需要先完成设备注册（已有认证会话），但如果受害者此前曾用该设备 ID 与攻击者交互过（攻击者控制服务器侧设备列表），则利用路径可行。

## 根因

Matrix 规范（CS API r0.6.1）中，`signed_curve25519` OTK 的签名来自 `device.ed25519` 私钥，用于证明"此 OTK 确实属于该设备"。如果设备声明本身不包含 `ed25519` 密钥（设备只有 curve25519 身份密钥），则无法建立这个信任链。

## 修复建议

**严格模式（推荐）**：当 `signed_curve25519` OTK 存在但设备无 `ed25519` 密钥时，直接拒绝上传（HTTP 400），与有 ed25519 密钥时行为一致：

```rust
} else {
    // 无 ed25519 设备密钥时，拒绝存储 signed_curve25519 OTK
    return Err(ApiError::bad_request(
        "Cannot store signed_curve25519 one-time key for device without ed25519 key",
    ));
}
```

**宽松模式**：若存在"无 ed25519 设备密钥"的合法场景（如某些只做密钥交换的只读设备），至少对无签名 OTK 做显式标记（单独的存储字段），并在 `claim_keys` 时排除无签名 OTK（除非在白名单路径）。

## 测试覆盖分析

现有测试 `upload_keys_rejects_invalid_signed_one_time_key` 先通过 `keys_to_upload_1` 种子数据写入 ed25519 设备密钥，因此覆盖的是"有 ed25519 → 签名校验失败"路径，未触发无 ed25519 的 else 分支。建议补充：
- `upload_keys_rejects_unsigned_otk_without_ed25519_key`：不种子 ed25519，直接上传 signed_curve25519 OTK，期望 400。

## 关联

- 相关签名校验逻辑：`synapse-e2ee/src/signed_json.rs:verify_one_time_key_signature`
- 对比：Federation 侧 `claim_keys` 同样依赖设备密钥签名；此漏洞可跨联邦利用。
