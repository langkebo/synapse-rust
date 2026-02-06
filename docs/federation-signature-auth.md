# Federation API 签名认证文档

## 概述

Synapse Rust 实现了 Matrix Federation 协议，用于不同 Matrix 服务器之间的通信。本文档详细说明了 Federation API 的签名认证要求和使用方法。

## 认证架构

### 签名方式

Matrix Federation 使用 Ed25519 数字签名算法进行请求认证：

- **算法**: Ed25519 (EdDSA)
- **密钥格式**: Base64 编码的 32 字节密钥
- **签名格式**: X-Matrix HTTP 头部

### 密钥结构

```rust
pub struct FederationTestKeypair {
    pub key_id: String,      // 格式: ed25519:<key_name>
    pub secret_key: String,   // Base64 编码的私钥
    pub public_key: String,  // Base64 编码的公钥
}
```

## API 端点分类

### 1. 公共端点（无需认证）

以下端点可以直接访问，无需签名认证：

| 端点 | 方法 | 描述 |
|------|------|------|
| `/_matrix/federation/v1/version` | GET | 获取 Federation 版本信息 |
| `/_matrix/federation/v1` | GET | Federation 服务器发现 |
| `/_matrix/federation/v2/server` | GET | 获取服务器密钥信息 |
| `/_matrix/key/v2/server` | GET | 获取服务器密钥信息 |
| `/_matrix/federation/v2/query/{server_name}/{key_id}` | GET | 查询特定密钥 |
| `/_matrix/key/v2/query/{server_name}/{key_id}` | GET | 查询特定密钥 |
| `/_matrix/federation/v1/publicRooms` | GET | 获取公共房间列表 |

**示例请求**:
```bash
curl http://localhost:8008/_matrix/federation/v1/version
```

**响应**:
```json
{
  "version": "synapse-rust"
}
```

### 2. 受保护端点（需要认证）

以下端点需要有效的 Federation 签名：

| 端点 | 方法 | 描述 |
|------|------|------|
| `/_matrix/federation/v1/members/{room_id}` | GET | 获取房间成员列表 |
| `/_matrix/federation/v1/joined_members/{room_id}` | GET | 获取加入的成员 |
| `/_matrix/federation/v1/rooms/{room_id}/state` | GET | 获取房间状态 |
| `/_matrix/federation/v1/rooms/{room_id}/state_ids` | GET | 获取状态事件 ID |
| `/_matrix/federation/v1/send/{txn_id}` | PUT | 发送事务 |
| `/_matrix/federation/v1/user/devices/{user_id}` | GET | 获取用户设备信息 |
| `/_matrix/federation/v1/keys/claim` | POST | 声明密钥 |
| `/_matrix/federation/v1/keys/upload` | POST | 上传密钥 |
| `/_matrix/federation/v1/get_missing_events/{room_id}` | POST | 获取缺失事件 |
| `/_matrix/federation/v1/send_join/{room_id}/{event_id}` | PUT | 发送加入事件 |
| `/_matrix/federation/v1/send_leave/{room_id}/{event_id}` | PUT | 发送离开事件 |

## 签名格式

### X-Matrix 请求头

所有认证请求必须在 HTTP 头部包含 `X-Matrix` 签名：

```
X-Matrix: origin=<origin_server>,destination=<destination_server>,key_id=<key_id>,sig=<base64_signature>
```

### 签名字符串格式

签名字符串按照以下格式构建：

```
<method>\n<path>\n<origin>\n<destination>\n<content>\n
```

**示例**:
```
GET
/_matrix/federation/v1/members/test_room
example.com
cjystx.top

```

### 生成签名

使用提供的工具生成测试签名：

```bash
# 生成测试密钥对
cargo run --bin generate_test_keypair
```

**示例输出**:
```
Key ID: ed25519:testAbCd12
Secret Key (base64): AAAA...BBBB
Public Key (base64): CCCC...DDDD

# Environment variables:
export FEDERATION_KEY_ID="ed25519:testAbCd12"
export FEDERATION_SECRET_KEY="AAAA...BBBB"
export FEDERATION_PUBLIC_KEY="CCC...DD"
```

### 签名函数

```rust
use synapse_rust::common::federation_test_keys::{
    generate_federation_test_keypair,
    sign_federation_request,
    verify_federation_signature
};

fn main() {
    // 生成密钥对
    let keypair = generate_federation_test_keypair();

    // 签名请求
    let signature = sign_federation_request(
        &keypair.secret_key,
        "GET",
        "/_matrix/federation/v1/members/test_room",
        "example.com",
        "cjystx.top",
        None,
    ).unwrap();

    println!("X-Matrix: {}", signature);

    // 验证签名
    let is_valid = verify_federation_signature(
        &keypair.public_key,
        "GET",
        "/_matrix/federation/v1/members/test_room",
        "example.com",
        "cjystx.top",
        None,
        &signature,
    ).unwrap();

    println!("Signature valid: {}", is_valid);
}
```

### Bash 签名示例

```bash
#!/bin/bash

# 配置
SECRET_KEY="your_secret_key_base64"
ORIGIN="example.com"
DEST="cjystx.top"
METHOD="GET"
PATH="/_matrix/federation/v1/members/test_room"

# 构建签名字符串
SIGNING_STRING="${METHOD}
${PATH}
${ORIGIN}
${DEST}
"

# 使用 openssl 进行 Ed25519 签名
# 注意: 需要将 Base64 密钥转换为十六进制

# 示例签名（简化）
SIGNATURE=$(echo -n "$SIGNING_STRING" | openssl dgst -sha512 -binary -sign <(echo "$SECRET_KEY" | base64 -d) | base64)

# 构建 X-Matrix 头部
HEADER="X-Matrix: origin=${ORIGIN},destination=${DEST},key_id=ed25519:1,sig=${SIGNATURE}"

# 发送请求
curl -H "$HEADER" \
     "http://localhost:8008${PATH}"
```

## 错误处理

### 认证失败响应

当请求缺少签名或签名无效时，服务器返回：

```json
{
  "errcode": "M_UNAUTHORIZED",
  "error": "Missing or invalid federation signature"
}
```

### 常见错误

| 错误码 | 描述 | 解决方案 |
|--------|------|----------|
| `M_UNAUTHORIZED` | 签名缺失或无效 | 检查 X-Matrix 头部格式 |
| `M_FORBIDDEN` | 密钥ID不匹配 | 确保使用正确的 key_id |
| `M_UNKNOWN` | 密钥不存在 | 检查密钥是否已注册 |
| `M_EXPIRED` | 签名已过期 | 检查时间戳（如果适用） |

## 密钥管理

### 测试环境

```rust
// tests/federation_auth.rs
use synapse_rust::common::federation_test_keys::FederationTestKeypair;

pub fn generate_test_keypair() -> FederationTestKeypair {
    FederationTestKeypair {
        key_id: "ed25519:test".to_string(),
        secret_key: generate_test_secret_key(),
        public_key: generate_test_public_key(),
    }
}
```

### 生产环境

在生产环境中，密钥应该：

1. **安全存储**: 使用密钥管理服务（KMS）
2. **定期轮换**: 定期生成新密钥并更新 DNS 记录
3. **备份**: 安全备份私钥
4. **监控**: 监控密钥使用情况

## DNS 配置

Federation 密钥需要在 DNS 中发布：

```dns
_matrix._tcp.example.com. 3600 IN SRV 10 0 8448 matrix.example.com.

# 公共密钥记录
_matrix._tcp.example.com. 3600 IN TXT "key_id=ed25519:1, fingerprint=..."
```

## 测试

### 运行签名认证测试

```bash
# 运行单元测试
cargo test federation_test_keys

# 运行集成测试
cargo test --test integration federation

# 运行完整测试套件
cargo test
```

### 测试脚本

提供了完整的测试脚本：

```bash
# Federation 签名认证测试
bash scripts/test_federation_auth.sh
```

## 安全性考虑

### 最佳实践

1. **不要在代码中硬编码密钥**
2. **使用环境变量或密钥管理服务**
3. **验证所有传入签名的服务器身份**
4. **实施密钥轮换策略**
5. **记录所有认证失败事件**

### 攻击防护

- **重放攻击**: 使用时间戳或事务 ID
- **中间人攻击**: 使用 TLS 和密钥验证
- **密钥泄露**: 立即轮换受影响密钥

## 参考

- [Matrix Federation Specification](https://matrix.org/docs/spec/server_server/latest)
- [Ed25519 Signature Algorithm](https://ed25519.cr.yp.to/)
- [Matrix Federation API](https://matrix.org/docs/spec/server_server/r0.1.4)
