# E-03: verify_backup 在无可用 device_key_storage 时降级为"仅检查 signatures 对象存在性"

## 严重等级

🟡 **Medium** — 降级路径使备份验证依赖"存在性"而非"密码学正确性"

## 涉及代码

- `synapse-e2ee/src/backup/service.rs:verify_backup`（行 ~150–170）

## 问题描述

`verify_backup` 的签名校验逻辑分为两条路径：

**路径 A（有 device_key_storage）**：
```rust
if let Some(device_key_storage) = &self.device_key_storage {
    let signature_valid = self
        .verify_backup_signatures(backup_data, device_key_storage)
        .await
        .unwrap_or(false);
    // ✓ 密码学验证
}
```

**路径 B（device_key_storage = None）**：
```rust
} else {
    let has_signatures = signatures
        .as_object()
        .is_some_and(|m| !m.is_empty());
    signature_valid = has_signatures;
    // ⚠ 仅检查 signatures 对象存在且非空，不做密码学验证
}
```

路径 B 的含义是"设备密钥存储不可用时的 fallback"，但 fallback 逻辑等于**完全跳过签名验证**：只要客户端提交了任意非空的 signatures 对象，`signature_valid = true`，攻击者可以用任何伪造签名通过验证。

## Matrix 规范要求

根据 Matrix 规范 [MSC2380](https://github.com/matrix-org/matrix-spec-proposals/pull/2380)，`/room_keys/version` 的 `auth_data` 必须由用户的主设备或其他已验证设备签名。验证应检查签名有效性，而非仅检查存在性。

## 影响场景

1. 服务器初始化时 `device_key_storage` 因配置问题被设置为 `None`。
2. 管理员调试/降级场景中禁用了 device_key_storage。
3. 客户端上传带有任意伪造 signatures 的备份（无正确密码学验证）。

在上述任一场景下，备份的完整性保证完全丧失。

## 修复建议

**选项 A（推荐）**：fallback 时保守拒绝，要求提供有效的设备签名：
```rust
} else {
    // 无 device_key_storage 时，安全拒绝而非降级
    tracing::warn!(
        "device_key_storage unavailable, cannot verify backup signatures"
    );
    return Ok(BackupVerifyResult {
        signature_valid: false,
        has_signatures: signatures.as_object().is_some_and(|m| !m.is_empty()),
        trusted_device_ids: vec![],
    });
}
```

**选项 B**：fallback 时仅检查 signatures 字段结构合理性（格式、长度），不做密码学验证但做降级告警：
```rust
} else {
    let has_signatures = signatures.as_object().is_some_and(|m| !m.is_empty());
    if has_signatures {
        tracing::warn!(
            "device_key_storage unavailable: accepting backup with existing signatures \
             but skipping cryptographic verification"
        );
    }
    signature_valid = false; // 不降级信任
}
```

## 测试覆盖

现有测试 `test_backup_key_storage_unavailable`（backup/service.rs）覆盖了 device_key_storage = None 场景，断言 `signature_valid = false`。但测试中的 signatures 为空对象（`{}`），未覆盖"有非空 signatures 但无 crypto 验证"的降级路径。建议补充：
- `verify_backup_with_fake_signatures_no_device_key_storage`：signatures 包含伪造签名，期望 `signature_valid = false`（在 device_key_storage = None 时也应拒绝）。

## 与 E-02 的关联

E-02（备份更新绕过校验）和 E-03（无 device_key_storage 时降级）组合：攻击者若能触发 device_key_storage 不可用场景，可绕过 E-03 的验证，而 E-02 允许在有认证会话时替换备份公钥，两者组合可实现完整的备份完整性破坏。
