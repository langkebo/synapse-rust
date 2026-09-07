# E2EE 端到端加密审计报告

> **审计时间**：2026-09-04
> **审计范围**：`synapse-e2ee` crate + `src/web/routes/e2ee/` + `src/web/routes/key_backup.rs`
> **审计模式**：仅代码审计 + 报告（未提交修复代码）
> **审计人**：CodeReviewExpert

## TL;DR

共发现 **7 个安全/可用性问题**，按严重等级：

| 等级 | 数量 | Issues |
|------|------|--------|
| 🔴 High | 1 | E-02（备份更新绕过校验） |
| 🟡 Medium | 5 | E-01、E-03、E-04、E-05、E-07 |
| 💡 Low / Informational | 1 | E-06（pickle key 持久化） |

**核心问题**：备份的 `update_backup_auth_data` 路径在写入前不重做签名校验（E-02），叠加 `verify_backup` 在 `device_key_storage` 不可用时降级为"仅检查 signatures 存在性"（E-03），形成完整的备份完整性破坏链。

**次要问题**：`DeviceKeyService.upload_keys` 在设备无 ed25519 密钥时静默接受未签名的 `signed_curve25519` 一次性密钥（E-01），存在 MITM 利用空间。

**架构层问题**：Olm/Megolm 服务端解密无 `message_index` 重放检测（E-04），与 `forward_keys_for_new_member` 无去重（E-07）联动，可导致 to-device 消息重放和密钥分发路径的流量放大。

## Issue 总览

| 编号 | 标题 | 严重等级 | 模块 |
|------|------|----------|------|
| E-01 | signed_curve25519 一次性密钥在无 ed25519 设备密钥时绕过签名校验 | 🟡 Medium-High | device_keys |
| E-02 | 备份更新（PUT /room_keys/version/{version}）不对 auth_data 做重新校验 | 🔴 High | backup |
| E-03 | verify_backup 在无可用 device_key_storage 时降级为"仅检查 signatures 对象存在性" | 🟡 Medium | backup |
| E-04 | Olm/Megolm 解密路径缺少服务器侧消息重放保护 | 🟡 Medium | olm / megolm |
| E-05 | 备份版本号解析在 KeyBackupStorage 中以 `i64::unwrap_or(0)` 兜底 | 🟡 Medium | backup |
| E-06 | OLM_PICKLE_KEY 默认随机生成，跨重启会丢失已 pickle 的 Olm/Megolm 会话 | 💡 Informational | olm |
| E-07 | 密钥轮换服务 forward_keys_for_new_member 缺少重放保护与速率限制 | 🟡 Medium | key_rotation |

## 详细发现

### E-01: signed_curve25519 OTK 在无 ed25519 设备密钥时绕过签名校验

**模块**：`synapse-e2ee/src/device_keys/service.rs`
**问题**：`upload_keys` 在处理 `signed_curve25519` 一次性密钥时，若设备声明中不存在 `ed25519` 密钥，则**仅 warn 后直接存储**，不进行任何签名验证。fallback_keys 路径同样有此问题。
**风险**：攻击者可上传没有 ed25519 密钥的设备声明（含 signed_curve25519 OTK），MITM 后续 1:1 会话建立过程。
**修复**：在两个 else 分支直接返回 400，拒绝存储。
[详细说明 →](./issues/E-01-unsigned-otk-no-ed25519.md)

### E-02: 备份更新（PUT /room_keys/version/{version}）不对 auth_data 做重新校验

**模块**：`synapse-e2ee/src/backup/service.rs:update_backup_auth_data`
**问题**：对比 `create_backup`（`validate_auth_data` + public_key 校验），`update_backup_auth_data` **直接将调用者提交的 auth_data 字段写入数据库**，无 re-validation、无签名重校验。
**风险**：攻击者替换备份的 `public_key` 为自己持有的密钥对，使后续从备份恢复的会话可被离线解密。
**修复**：在写入前重新调用 `validate_auth_data` + 校验 `mgmt_key` 未变 + 校验 `auth_data.signatures`。
[详细说明 →](./issues/E-02-backup-auth-data-update-no-revalidation.md)

### E-03: verify_backup 降级为"仅检查 signatures 对象存在性"

**模块**：`synapse-e2ee/src/backup/service.rs:verify_backup`
**问题**：`verify_backup` 在 `device_key_storage = None` 时，**不调用密码学验证**，而仅检查 `signatures.as_object().is_some_and(|m| !m.is_empty())`。任何伪造的 signatures 都会通过验证。
**风险**：若服务器配置错误导致 device_key_storage 不可用，备份签名验证完全失效。
**修复**：降级路径应保守拒绝（`signature_valid = false`），或至少发出告警 + 强制重新启用 device_key_storage。
[详细说明 →](./issues/E-03-verify-backup-no-crypto-fallback.md)

### E-04: Olm/Megolm 服务端解密无 message_index 重放检测

**模块**：`synapse-e2ee/src/olm/session.rs:decrypt` 和 `synapse-e2ee/src/vodozemac_megolm.rs:decrypt`
**问题**：vodozemac 0.9 的 Olm/Megolm ratchet 自身对**前向**消息保护良好，但**不主动拒绝已见过 message_index 的重放**。服务端 `decrypt` 不显式跟踪 `message_index`，无单调性检查。
**风险**：to-device 消息重放、ratchet state 回退攻击（旧密文再次可解密）。
**修复**：服务端在 `decrypt` 后校验 `message_index > last_index`，并在存储中记录已见 index。
[详细说明 →](./issues/E-04-no-replay-protection-on-decrypt.md)

### E-05: 备份版本号 i64 解析的 `unwrap_or(0)` 反模式

**模块**：`synapse-e2ee/src/backup/storage.rs:get_backup_version`
**问题**：`version.parse::<i64>().unwrap_or(0)` 在非数字版本号时静默降级为 0，可能误匹配实际版本。
**风险**：版本号解析不一致导致查询命中错误的备份。
**修复**：用 `Option<i64>` 区分，纯数字走 numeric 路径，字符串走 string 路径。
[详细说明 →](./issues/E-05-backup-version-i64-unwrap-or-zero.md)

### E-06: OLM_PICKLE_KEY 默认随机生成

**模块**：`synapse-e2ee/src/olm/service.rs:get_pickle_key`
**问题**：`OLM_PICKLE_KEY` 环境变量未设置时使用 CSPRNG 随机生成 32 字节 key，**每个进程独立**。重启后旧 pickle 无法解密。
**风险**：可用性问题（用户被迫重新协商会话）；间接安全性问题（重新建立密钥路径消耗 OTK、增大密钥分发暴露面）。
**修复**：生产环境 fail-fast；支持 `OLM_PICKLE_KEY_FILE` 路径配置；长期集成 KMS。
[详细说明 →](./issues/E-06-pickle-key-default-random-reboot-loss.md)

### E-07: forward_keys_for_new_member 无去重与速率限制

**模块**：`synapse-e2ee/src/key_rotation/service.rs:forward_keys_for_new_member`
**问题**：新成员加入加密房间时，发送 `m.room_key` to-device 消息**前不查询是否已分享过**，同一 (user, device, room) 可被反复触发。
**风险**：to-device 流量放大攻击；与 E-04 联动，攻击者可截获并重放 room_key。
**修复**：在 `megolm_key_shares` 表加唯一约束；分享前先查重。
[详细说明 →](./issues/E-07-key-rotation-no-dedup-on-forward.md)

## 修复优先级建议

### P0（48 小时内修复）

- **E-02**：备份更新绕过校验是最直接的高危路径，影响备份完整性。
- **E-01**：未签名 OTK 静默存储是 E2EE 信任链的基础漏洞。

### P1（一周内修复）

- **E-03**：降级路径的"存在性即正确"语义错误。
- **E-07**：密钥分发路径的去重缺失可被流量放大。

### P2（下次 Sprint 处理）

- **E-04**：message_index 重放保护，需要数据库 schema 升级。
- **E-05**：版本号解析反模式，修复点很小但需要回归测试。
- **E-06**：pickle key 持久化是工程化问题，建议在配置文档中显式标注。

## 与历史审计的关联

| 联邦审计 Issue | E2EE 关联 Issue | 关联说明 |
|----------------|------------------|----------|
| F-05（联邦签名校验） | E-01 | 联邦 `claim_keys` 路径同样依赖设备签名；E-01 修复后联邦路径也受益 |
| F-04（联邦 key fetch） | E-01 | 联邦路径读取的设备密钥如果是 unsigned，则整个联邦信任链断裂 |
| F-02（联邦事件哈希） | E-04 | 若联邦事件流被重放，本地 Olm 解密路径需要发现异常 |

## 测试覆盖建议

针对发现的问题，建议在 `synapse-e2ee/src/` 下补充以下单元测试：

```
// device_keys/service.rs
test_upload_keys_rejects_unsigned_otk_without_ed25519_key      // E-01
test_upload_keys_rejects_unsigned_fallback_without_ed25519_key  // E-01
test_upload_keys_accepts_unsigned_otk_when_ed25519_present      // E-01 阳性对照

// backup/service.rs
test_update_backup_auth_data_revalidates_signature             // E-02
test_update_backup_auth_data_rejects_mgmt_key_change            // E-02
test_verify_backup_rejects_with_fake_signatures_no_storage     // E-03

// olm/session.rs
test_olm_decrypt_rejects_replayed_message_index                // E-04
test_olm_decrypt_detects_index_regression                      // E-04

// vodozemac_megolm.rs
test_megolm_decrypt_rejects_replayed_message_index             // E-04
test_megolm_decrypt_detects_index_regression                   // E-04

// backup/storage.rs
test_get_backup_version_with_uuid_no_panic                     // E-05
test_get_backup_version_with_negative_string                   // E-05

// key_rotation/service.rs
test_forward_keys_dedups_existing_share                        // E-07
test_forward_keys_rate_limits_repeated_calls                   // E-07
```

## 审计方法学

本次审计采用**代码静态审计**（无运行时验证），重点关注：

1. **信任链**：从用户 → 设备 → 会话 → 消息 → 备份，每一环节的签名/认证机制是否完整。
2. **降级路径**：当主路径失败或不可用时（如 device_key_storage = None），fallback 是否引入了新的安全漏洞。
3. **持久化语义**：pickle、storage 层的错误处理（best-effort vs strict）。
4. **跨模块联动**：单一漏洞是否可与其他 Issue 组合形成完整攻击链（如 E-02 + E-03 备份完整性破坏、E-04 + E-07 流量放大）。
5. **测试覆盖盲点**：通过审阅测试文件，识别未触发的代码路径（如 E-01 的 else 分支）。

## 未审计范围

- **密钥派生（KDF）**：未深入审计 vodozemac 内部 KDF 路径（信任上游库）。
- **随机数生成**：vodozemac 内部使用 `getrandom`，未审计平台 RNG 实现。
- **侧信道**：未审计 timing attack、cache-timing 等侧信道。
- **CRUD 权限**：设备密钥 CRUD 的访问控制（已有 RBAC 层，未深入审计 RBAC 配置本身）。
- **联邦路径的密钥分发**：联邦侧的 E2EE 密钥分发（claim_keys across federation）由 FederationClient 处理，详见 Federation Audit Report。

## 后续工作

- 本次审计仅产出报告，不包含代码修复。
- 每个 Issue 建议作为独立 ticket 跟踪修复进度。
- 修复后建议重跑 `cargo test --features test-utils --test integration e2ee*` 验证回归。
- P0 修复建议单独提交 commit + PR review。

## 附录：相关文件清单

### 核心审计文件

- `synapse-e2ee/src/lib.rs` — 模块声明
- `synapse-e2ee/Cargo.toml` — 依赖（vodozemac 0.9, ed25519-dalek 2.0 等）
- `synapse-e2ee/src/olm/service.rs` — OlmService + pickle key
- `synapse-e2ee/src/olm/session.rs` — OlmSessionManager（in-memory + decrypt）
- `synapse-e2ee/src/olm/storage.rs` — olm_accounts/olm_sessions 表
- `synapse-e2ee/src/megolm/service.rs` — MegolmProvider 薄壳
- `synapse-e2ee/src/vodozemac_megolm.rs` — 实际 Megolm 实现（含 ratchet + decrypt）
- `synapse-e2ee/src/megolm/models.rs` — MegolmSession, PickleFormat
- `synapse-e2ee/src/backup/service.rs` — KeyBackupService（含 E-02/E-03 漏洞）
- `synapse-e2ee/src/backup/storage.rs` — KeyBackupStorage（含 E-05）
- `synapse-e2ee/src/cross_signing/service.rs` — CrossSigningService
- `synapse-e2ee/src/device_keys/service.rs` — DeviceKeyService（含 E-01 漏洞）
- `synapse-e2ee/src/device_keys/storage.rs` — DeviceKeyStorage
- `synapse-e2ee/src/key_rotation/service.rs` — KeyRotationService（含 E-07 漏洞）
- `synapse-e2ee/src/signed_json.rs` — 签名校验基础设施

### 路由层

- `src/web/routes/e2ee/keys.rs` — upload_keys, query_keys, claim_keys, key_changes
- `src/web/routes/key_backup.rs` — /room_keys/* 端点
- `src/web/routes/e2ee/devices.rs` — 设备管理
- `src/web/routes/key_rotation.rs` — 密钥轮换 API

### 测试文件

- `synapse-e2ee/src/backup/service.rs` 内的 `mod tests`
- `synapse-e2ee/src/cross_signing/service.rs` 内的 `mod tests`
- `tests/integration/e2ee_*` 系列集成测试

### 配置项

- `OLM_PICKLE_KEY`（32 字节 hex）— E-06
- `E2EE_DUAL_WRITE` — Megolm 迁移期间的双写开关
- `MEGOLM_SESSION_MAX_AGE_DAYS` — Megolm session 过期（默认 7 天）

---

*本报告由 CodeReviewExpert 自动生成，基于 2026-09-04 上午的代码审计会话。*
