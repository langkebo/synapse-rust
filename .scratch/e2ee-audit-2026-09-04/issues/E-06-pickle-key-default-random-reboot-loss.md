# E-06: OLM_PICKLE_KEY 默认随机生成，跨重启会丢失已 pickle 的 Olm/Megolm 会话

## 严重等级

🟡 **Medium** — 影响可用性 + 间接影响安全性（用户被迫重新建立密钥）

## 涉及代码

- `synapse-e2ee/src/olm/service.rs:get_pickle_key`（行 ~30–60）

## 问题描述

```rust
// synapse-e2ee/src/olm/service.rs
fn get_pickle_key() -> Result<[u8; 32], ApiError> {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    if let Some(k) = KEY.get() {
        return Ok(*k);
    }
    let key = match std::env::var("OLM_PICKLE_KEY") {
        Ok(hex) => {
            let bytes = hex::decode(&hex).map_err(...)?;
            bytes.try_into().map_err(...)
        }
        Err(_) => {
            // ⚠ 随机生成
            let mut key = [0u8; 32];
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut key);
            tracing::warn!(
                "OLM_PICKLE_KEY not set, using random key. \
                 Pickled sessions will NOT survive server restart."
            );
            Ok(key)
        }
    }?;
    KEY.get_or_init(|| key);
    Ok(key)
}
```

## 风险分析

### 安全性

- **每个进程重新生成**：新进程启动时生成新 key，旧的 pickle 密文无法解密。
- **不影响在线安全**：加密本身安全，vodozemac 0.9 + AES-GCM 正确。
- **间接影响**：跨重启会话丢失，用户被迫：
  1. 重新协商 Olm 会话（消耗新 OTK）
  2. 重新分发 Megolm 会话（消耗带宽 + 增加密钥分发路径暴露面）
  3. 备份恢复不可用（如果新进程的 pickle key 未知）

### 可用性

- **生产环境**：`OLM_PICKLE_KEY` 未设置是高风险，会导致会话频繁重建。
- **开发环境**：单次重启丢失所有会话是常见绊脚石。

### 攻击面

- **DoS**：若 OLM_PICKLE_KEY 被误清空，攻击者需从 0 重建所有密钥路径（耗费 OTK、加重密钥分发）。
- **密钥外泄风险**：随机生成 32 字节 entropy 充足（CSPRNG），单实例下安全；但多实例部署时，实例 A 的 pickle 在实例 B 无法解密。

## 修复建议

### 短期：fail-fast 在生产环境

```rust
Err(_) => {
    if cfg!(not(debug_assertions)) {
        return Err(ApiError::internal_error(
            "OLM_PICKLE_KEY must be set in production",
        ));
    }
    tracing::warn!("OLM_PICKLE_KEY not set, using random key (dev only)");
    // ... random fallback
}
```

### 中期：自动从 secrets file 加载

支持 `OLM_PICKLE_KEY_FILE` 路径配置，启动时读取：

```rust
let key = match std::env::var("OLM_PICKLE_KEY") {
    Ok(hex) => parse_hex(&hex)?,
    Err(_) => {
        if let Ok(path) = std::env::var("OLM_PICKLE_KEY_FILE") {
            let hex = std::fs::read_to_string(path)?;
            parse_hex(&hex.trim())?
        } else {
            random_fallback()
        }
    }
};
```

### 长期：集成 KMS / Vault

将 pickle key 委托给外部 KMS 托管，避免明文落盘到 env。

## 文档与配置

需在 `config.example.toml` / `DEPLOYMENT.md` 中明确：
- `OLM_PICKLE_KEY` 必须为 64 字符 hex（32 字节）
- 跨重启、跨实例必须使用相同值
- 建议使用 `openssl rand -hex 32` 生成
- 建议使用 secrets manager 托管

## 与其他 Issue 的关联

- **E-04（重放保护）**：pickle 状态包含 ratchet state；pickle 丢失会强制 ratchet 重建，间接使 E-04 的 message_index 跟踪从 0 重新开始。
- **F-04（联邦 pickle）**：联邦侧 server_keys 也用类似机制，需统一治理。
