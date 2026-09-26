# ED25519 签名和内容哈希计算实现指南 (v12 规范)

> 目标：实现完全符合 Matrix Room Version 12 规范的 ED25519 签名和内容哈希计算
> 参考：Synapse v1.161 `synapse/crypto/event_signing.py` + Matrix Spec v1.18

---

## 1. 核心算法概述

### 1.1 内容哈希 (`hashes.sha256`)

**算法步骤**:

1. **移除临时字段**: 从事件中删除 `age_ts`, `unsigned`, `signatures`, `hashes`, `outlier`, `destinations`
2. **Canonical JSON**: 将剩余字段编码为 Matrix canonical JSON（按键名 ASCII 排序）
3. **SHA-256 哈希**: 计算 SHA-256 摘要
4. **Base64 编码**: 使用**标准** Base64 字母表（`+/`），无填充

**关键细节**:
- 内容哈希是对**未红事**件计算的（与签名材料不同）
- 使用标准 Base64 字母表（不是 URL-safe）
- 参考上游已知答案向量（见第 4 节测试）

### 1.2 ED25519 签名 (`signatures.<server>.<key_id>`)

**算法步骤**:

1. **准备签名材料**: 从事件中删除 `signatures` 和 `unsigned`
2. **Canonical JSON**: 编码为 canonical JSON
3. **ED25519 签名**: 使用服务器的私钥签名
4. **Base64 编码**: 使用**标准** Base64 字母表，无填充
5. **插入签名**: 将签名插入 `signatures.<server_name>.<key_id>`

**关键细节**:
- 签名材料不包括 `signatures` 和 `unsigned` 字段
- 可以有多重签名（多个服务器或多个密钥）
- 签名不会覆盖已有的其他服务器签名

### 1.3 事件 ID (Room Version 3+)

**算法步骤**:

1. **红事**件：应用房间版本的红事**件算法**
2. **移除临时字段**: 删除 `signatures`, `unsigned`, `age_ts`
3. **Canonical JSON**: 编码为 canonical JSON
4. **SHA-256 哈希**: 计算 SHA-256 摘要
5. **Base64 编码**: 
   - Room v3: 标准 Base64 (`+/`)
   - Room v4+: URL-safe Base64 (`-_`)
6. **添加前缀**: 添加 `$` 前缀

**关键区别**:
- Room v3 使用标准 Base64（可能包含 `/` 和 `+`）
- Room v4+ 使用 URL-safe Base64（包含 `-` 和 `_`）
- 参考哈希本身相同，只是编码字母表不同

---

## 2. 现有实现位置

### 2.1 内容哈希

**文件**: `synapse-federation/src/signing.rs`

```rust
/// 计算事件内容哈希（已实现，符合规范）
pub fn compute_event_content_hash(event: &Value) -> Option<String> {
    // 移除临时字段
    let mut stripped = event.clone();
    let obj = stripped.as_object_mut()?;
    for key in ["age_ts", "unsigned", "signatures", "hashes", "outlier", "destinations"] {
        obj.remove(key);
    }
    // Canonical JSON + SHA-256 + Base64
    let canonical = canonical_json(&stripped).ok()?;
    use sha2::Digest;
    let hash = sha2::Sha256::digest(canonical.as_bytes());
    Some(base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash))
}
```

**验证测试** (符合上游已知答案向量):
- `content_hash_matches_synapse_known_answer_minimal`: `"mq4QfPPpC+QsBd6eqfVsmJIEz8uvMSVK0+AU67PLESk"`
- `content_hash_matches_synapse_known_answer_message`: `"rDCeYBepPlI891h/RkI2/Lkf9bt7u0TxFku4tMs7WKk"`

### 2.2 ED25519 签名

**文件**: `synapse-federation/src/signing.rs`

```rust
/// 签署 JSON 对象（已实现，符合规范）
pub fn sign_json(server_name: &str, key_id: &str, secret_key_base64: &str, value: &mut Value) -> Result<(), String> {
    let canonical = CanonicalEvent::from_event(value).map_err(|e| format!("Canonical JSON error: {e}"))?;
    sign_json_with_canonical(server_name, key_id, secret_key_base64, value, &canonical)
}

/// 使用预计算的 CanonicalEvent 签名（避免重复排序）
pub fn sign_json_with_canonical(
    server_name: &str,
    key_id: &str,
    secret_key_base64: &str,
    value: &mut Value,
    canonical: &CanonicalEvent,
) -> Result<(), String> {
    let unsigned = canonical.canonical_bytes();
    
    // 解码私钥并签名
    let secret_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(secret_key_base64)
        .map_err(|e| format!("Invalid secret key base64: {e}"))?
        .try_into()
        .map_err(|_| "Secret key must be 32 bytes".to_string())?;
    
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let signature = signing_key.sign(unsigned);
    let sig_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signature.to_bytes());
    
    // 插入签名到 JSON
    // ...
}
```

### 2.3 组合函数：签名并设置哈希

**文件**: `synapse-federation/src/signing.rs`

```rust
/// 为本地生成的 PDU 设置内容哈希并添加服务器签名
pub fn sign_and_hash_event(
    server_name: &str,
    key_id: &str,
    secret_key_base64: &str,
    event: &mut Value,
) -> Result<(), String> {
    // 1. 确保 origin 字段设置
    // 2. 计算并设置 hashes.sha256
    // 3. 添加 ED25519 签名
    // ...
}
```

### 2.4 事件 ID 计算 (Reference Hash)

**文件**: `synapse-common/src/event_id.rs`

```rust
/// 计算事件的参考哈希（Room v3+）
pub fn compute_reference_hash(room_version: &str, event: &Value) -> Result<[u8; 32], EventIdError> {
    // 1. 红事**件
    let mut redacted = redact_event(room_version, event)?;
    // 2. 移除临时字段
    if let Some(obj) = redacted.as_object_mut() {
        obj.remove("signatures");
        obj.remove("unsigned");
        obj.remove("age_ts");
    }
    // 3. Canonical JSON + SHA-256
    let canonical = canonical_json(&redacted).map_err(|e| EventIdError::CanonicalJson(e.to_string()))?;
    Ok(Sha256::digest(canonical.as_bytes()).into())
}

/// 编码事件 ID
pub fn encode_reference_hash_event_id(room_version: &str, hash: &[u8]) -> Result<String, EventIdError> {
    let encoded = if room_version == "3" { 
        STANDARD_NO_PAD.encode(hash) 
    } else { 
        URL_SAFE_NO_PAD.encode(hash) 
    };
    Ok(format!("${encoded}"))
}

/// 完整的事件 ID 计算
pub fn compute_event_id(room_version: &str, event: &Value) -> Result<String, EventIdError> {
    let hash = compute_reference_hash(room_version, event)?;
    encode_reference_hash_event_id(room_version, &hash)
}
```

**验证测试** (符合上游已知答案向量):
- Room v10: `"$zRz9jjiT9wZc3Hl9ij_74aCmTjqV3YMlj9sj3Uqxg6o"`
- Room v3: `"$zRz9jjiT9wZc3Hl9ij/74aCmTjqV3YMlj9sj3Uqxg6o"`

---

## 3. 待完成的接线工作

### 3.1 当前状态

- ✅ 内容哈希算法已实现并通过上游已知答案向量测试
- ✅ ED25519 签名算法已实现并通过测试
- ✅ 事件 ID 参考哈希算法已实现并通过测试
- ⚠️ **局部使用**: 大部分事件创建仍使用旧的 `crypto::generate_event_id`（随机格式）

### 3.2 需要切换的调用点

以下调用点需要根据房间版本切换到 `event_id::compute_event_id`:

**高优先级** (出站 PDU 生成):
1. `synapse-services/src/room/messaging/events.rs` - 消息发送
2. `synapse-services/src/room/lifecycle/create_events.rs` - 房间创建
3. `synapse-web/src/routes/federation/transaction.rs` - 联邦事务
4. `synapse-web/src/routes/federation/membership/invite.rs` - 邀请
5. `synapse-web/src/routes/federation/membership/knock.rs` - 敲门

**模式**:
```rust
// ❌ 旧方式 (始终生成随机 ID)
let event_id = synapse_common::crypto::generate_event_id(&server_name);

// ✅ 新方式 (根据房间版本选择算法)
use synapse_common::event_id::{compute_event_id, uses_reference_hash_event_id};

let event_id = if uses_reference_hash_event_id(&room_version) {
    compute_event_id(&room_version, &event_without_id)?
} else {
    crypto::generate_event_id(&server_name)  // v1/v2 仍用随机 ID
};
```

### 3.3 实施步骤

#### 步骤 1: 创建辅助函数

在 `synapse-common/src/lib.rs` 添加:

```rust
/// 根据房间版本计算事件 ID
/// Room v1/v2: 随机 ID (兼容旧格式)
/// Room v3+: 参考哈希 ID (规范格式)
pub fn compute_event_id_for_room_version(
    room_version: &str,
    event: &serde_json::Value,
    server_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if event_id::uses_reference_hash_event_id(room_version) {
        Ok(event_id::compute_event_id(room_version, event)?)
    } else {
        Ok(crypto::generate_event_id(server_name))
    }
}
```

#### 步骤 2: 更新调用点

逐个更新上述调用点，确保：
1. 在计算事件 ID 之前准备好事件的所有其他字段
2. 传递正确的房间版本
3. 处理可能的错误（`EventIdError`）

#### 步骤 3: 集成测试

验证生成的 PDU 可以被其他 Synapse 实例接受：
- 检查 `event_id` 格式是否符合预期
- 验证 `hashes.sha256` 可以被重新计算并匹配
- 验证 `signatures` 可以通过 ED25519 验证

---

## 4. 测试验证

### 4.1 已知答案向量测试

现有测试已覆盖上游已知答案向量：

```rust
// synapse-federation/src/signing.rs
#[test]
fn content_hash_matches_synapse_known_answer_minimal() {
    let event = serde_json::json!({
        "event_id": "$0:domain",
        "origin_server_ts": 1000000,
        "signatures": {},
        "type": "X",
        "content": {},
        "unsigned": {"age_ts": 1000000},
    });
    assert_eq!(compute_event_content_hash(&event).as_deref(), 
               Some("mq4QfPPpC+QsBd6eqfVsmJIEz8uvMSVK0+AU67PLESk"));
}

// synapse-common/src/event_id.rs
#[test]
fn synapse_known_answer_vector_v10() {
    let event = synapse_vector_event();
    assert_eq!(compute_event_id("10", &event).unwrap(), 
               "$zRz9jjiT9wZc3Hl9ij_74aCmTjqV3YMlj9sj3Uqxg6o");
}
```

### 4.2 变异自证

为确保实现正确，构造以下变异测试：

1. **内容哈希变异**:
   - 修改事件内容 → 哈希应该改变
   - 添加/删除 `unsigned` 字段 → 哈希应该不变
   - 使用不同的 Base64 字母表 → 哈希字符串应该不同

2. **签名变异**:
   - 篡改事件内容 → 签名验证应该失败
   - 使用错误的私钥 → 签名验证应该失败
   - 修改签名后的事件 → 验证应该失败

3. **事件 ID 变异**:
   - 修改受保护的字段（如 `depth`）→ 事件 ID 应该改变
   - 修改不受保护的字段（如内容）→ 事件 ID 应该不变（红事后）
   - 使用错误的房间版本 → 事件 ID 编码应该不同（v3 vs v4）

---

## 5. 注意事项

### 5.1 Base64 字母表

| 用途 | 字母表 | 填充 | 示例字符 |
|------|--------|------|----------|
| 内容哈希 | 标准 (`+/`) | 无 | `mq4QfPPpC+Qs` |
| 事件 ID v3 | 标准 (`+/`) | 无 | `.../...+...` |
| 事件 ID v4+ | URL-safe (`-_`) | 无 | `..._...-...` |
| 签名 | 标准 (`+/`) | 无 | `G8cfk/m97snd` |

### 5.2 红事**件影响

- **内容哈希**: 对**未红事**件计算（包括完整内容）
- **签名材料**: 对**未红事**件计算（包括完整内容）
- **事件 ID**: 对**红事**件计算（内容被剥离）

### 5.3 多重签名

- 一个事件可以有多个服务器的签名
- 一个服务器可以有多个密钥的签名（密钥轮换）
- `sign_json` 会**添加**签名，不会覆盖现有签名

### 5.4 错误处理

所有函数都返回 `Result` 或 `Option`：
- `compute_event_content_hash`: `Option<String>` (失败返回 `None`)
- `sign_json`: `Result<(), String>` (失败返回错误信息)
- `compute_event_id`: `Result<String, EventIdError>` (详细错误类型)

---

## 6. 参考链接

- Matrix Spec v1.18: [Event Formats](https://spec.matrix.org/v1.18/client-server-api/#event-format)
- Matrix Spec v1.18: [Room Versions](https://spec.matrix.org/v1.18/rooms/v12/)
- Synapse v1.161: `synapse/crypto/event_signing.py`
- Synapse v1.161: `synapse/events/utils.py`
- Element-HQ/Synapse: `tests/crypto/test_event_signing.py`

---

## 7. 实施检查清单

- [x] 内容哈希算法实现并测试
- [x] ED25519 签名算法实现并测试
- [x] 事件 ID 参考哈希算法实现并测试
- [ ] 创建辅助函数 `compute_event_id_for_room_version`
- [ ] 更新所有事件创建调用点
- [ ] 集成测试（与其他 Synapse 实例互操作）
- [ ] 文档更新（说明 v12 规范符合性）
