# E-02: 密钥备份更新（PUT /room_keys/version/{version}）不对 auth_data 做重新校验

## 严重等级

🔴 **High** — 攻击者可替换备份公钥，破坏备份完整性（confused deputy）

## 涉及代码

- `synapse-e2ee/src/backup/service.rs:update_backup_auth_data`（行 ~120–145）
- `src/web/routes/key_backup.rs:update_backup_version`（行 ~180）

## 问题描述

`KeyBackupService.create_backup`（POST /room_keys/version）会校验 `auth_data` 必须包含 `public_key`（通过 `validate_auth_data`），确保备份密钥由受信任的设备签名。

但 `update_backup_auth_data`（PUT /room_keys/version/{version}）**仅将调用者提供的 auth_data 原样写入**，未做以下校验：

1. `auth_data.mgmt_key` 与请求者提交的 `version.auth_data.mgmt_key` 是否一致（防止其他人替换 mgmt_key）
2. `auth_data.public_key` 是否仍然由原设备私钥签名
3. `auth_data.signatures` 是否有效（防止公钥被替换）

```rust
// key_backup.rs:update_backup_version
pub async fn update_backup_version(
    State(state): State<AppState>,
    Path(version): Path<String>,
    Json(auth_data): Json<Value>,
) -> Result<Json<serde_json::Value>, Report> {
    let auth_data = state
        .key_backup_service
        .update_backup_auth_data(&version, user_id, auth_data, &state)
        .await?;
    // ...
}

// backup/service.rs:update_backup_auth_data（约行 120）
pub async fn update_backup_auth_data(
    &self,
    version: &str,
    user_id: &UserId,
    auth_data: Value,
    state: &AppState,
) -> ApiResult<Value> {
    let mut updated_backup = self.storage.get_backup_version(version, user_id).await?
        .ok_or_else(|| ApiError::not_found("Backup version not found"))?;

    // ⚠ 直接接受 auth_data 中的任何值
    if let Some(data) = auth_data.get("auth_data") {
        updated_backup.auth_key = data.get("auth_key").and_then(|v| v.as_str()).map(String::from);
        updated_backup.mgmt_key = data.get("mgmt_key").and_then(|v| v.as_str()).map(String::from);
        updated_backup.public_key = data.get("public_key").and_then(|v| v.as_str()).map(String::from);
        updated_backup.backup_data = Some(data.clone());
    }

    // ❌ 没有 validate_auth_data() 调用
    // ❌ 没有签名重校验
    self.storage.update_backup(version, user_id, &updated_backup).await?;
    Ok(serde_json::json!({ "version": version }))
}
```

对比 `create_backup`：
```rust
// backup/service.rs:create_backup（约行 50）
if let Some(auth_data) = auth_data {
    self.validate_auth_data(&auth_data)?;  // ✓ 校验 public_key 存在
    // ...
}
```

## 利用场景（Exploit Scenario）

1. 受害者创建备份，auth_data 包含可信的 `public_key=P_victim`（由受害者设备签名）。
2. 攻击者（以受害者身份）调用 `PUT /room_keys/version/{version}`，将 `public_key` 替换为 `P_attacker`（攻击者持有的私钥对应）。
3. 受害者下次通过该备份恢复时，用 `P_attacker` 解密 session_data，但攻击者拥有对应私钥，可离线解密所有备份消息。

## 修复建议

`update_backup_auth_data` 应在写入前重新校验：

```rust
// backup/service.rs
pub async fn update_backup_auth_data(
    &self,
    version: &str,
    user_id: &UserId,
    auth_data: Value,
    state: &AppState,
) -> ApiResult<Value> {
    let mut updated_backup = self.storage.get_backup_version(version, user_id).await?
        .ok_or_else(|| ApiError::not_found("Backup version not found"))?;

    if let Some(data) = auth_data.get("auth_data") {
        // ✓ 重新校验 auth_data 的 public_key 存在性
        self.validate_auth_data(data)?;

        // ✓ 校验 mgmt_key 未被篡改（如果实现了 mgmt_key 签名校验）
        let new_mgmt_key = data.get("mgmt_key").and_then(|v| v.as_str());
        if new_mgmt_key != updated_backup.mgmt_key.as_deref() {
            return Err(ApiError::bad_request(
                "Backup management key cannot be changed via update",
            ));
        }

        // ✓ 校验签名（如果 auth_data 包含 signatures）
        if let Some(sigs) = data.get("signatures").as_object() {
            if !sigs.is_empty() {
                self.verify_backup_auth_data_signatures(data, state).await?;
            }
        }

        updated_backup.auth_key = data.get("auth_key").and_then(|v| v.as_str()).map(String::from);
        updated_backup.mgmt_key = new_mgmt_key.map(String::from);
        updated_backup.public_key = data.get("public_key").and_then(|v| v.as_str()).map(String::from);
        updated_backup.backup_data = Some(data.clone());
    }

    self.storage.update_backup(version, user_id, &updated_backup).await?;
    Ok(serde_json::json!({ "version": version }))
}
```

**注**：若 Matrix 规范允许更新时变更公钥（用户换设备），则至少需要签名重校验来防止"攻击者替换公钥"攻击。

## 风险评级说明

- 若 `mgmt_key` 通过 HTTPS + 认证会话保护（非 Matrix 签名），则实际风险降低。
- 若 `auth_data.signatures` 在更新时会被客户端重新签名提交，则当前漏洞利用依赖"攻击者持有受害者会话"前提。
- 综合：High（有潜力破坏备份完整性的 confused deputy）。
