# Synapse Rust E2EE 实现指南

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **文档类型**：实现指南  
> **优先级**：高  
> **参考文档**：[E2EE 架构设计](./e2ee-architecture.md)

---

## 一、实现概述

### 1.1 实现目标

本指南提供端到端加密（E2EE）功能的详细实现步骤，包括：

**核心功能实现**：
- 设备密钥管理（Device Key Management）
- 跨签名密钥管理（Cross-Signing Key Management）
- Megolm 加密服务（Megolm Encryption Service）
- 密钥备份服务（Key Backup Service）
- 事件签名服务（Event Signature Service）

**技术实现**：
- Rust 加密库集成
- 数据库表设计和迁移
- API 端点实现
- 缓存策略实现
- 测试用例编写

### 1.2 实现路线图

```
阶段 1：基础设施 (Week 1-2)
├── 项目结构搭建
├── 依赖库集成
├── 数据库表创建
└── 基础工具函数

阶段 2：密钥管理 (Week 3-4)
├── 设备密钥服务
├── 跨签名密钥服务
├── 密钥存储层
└── 密钥缓存

阶段 3：加密服务 (Week 5-6)
├── Megolm 服务
├── 加密/解密接口
├── 会话管理
└── 密钥分发

阶段 4：备份与签名 (Week 7-8)
├── 密钥备份服务
├── 事件签名服务
├── 密钥恢复
└── 签名验证

阶段 5：API 集成 (Week 9-10)
├── API 端点实现
├── 请求/响应处理
├── 错误处理
└── 文档生成

阶段 6：测试与优化 (Week 11-12)
├── 单元测试
├── 集成测试
├── 性能测试
└── 安全审计
```

---

## 二、项目结构搭建

### 2.1 目录结构

```
synapse_rust/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── e2ee/
│   │   ├── mod.rs
│   │   ├── crypto/
│   │   │   ├── mod.rs
│   │   │   ├── ed25519.rs
│   │   │   ├── x25519.rs
│   │   │   ├── aes.rs
│   │   │   └── argon2.rs
│   │   ├── device_keys/
│   │   │   ├── mod.rs
│   │   │   ├── service.rs
│   │   │   ├── storage.rs
│   │   │   └── models.rs
│   │   ├── cross_signing/
│   │   │   ├── mod.rs
│   │   │   ├── service.rs
│   │   │   ├── storage.rs
│   │   │   └── models.rs
│   │   ├── megolm/
│   │   │   ├── mod.rs
│   │   │   ├── service.rs
│   │   │   ├── storage.rs
│   │   │   └── models.rs
│   │   ├── backup/
│   │   │   ├── mod.rs
│   │   │   ├── service.rs
│   │   │   ├── storage.rs
│   │   │   └── models.rs
│   │   ├── signature/
│   │   │   ├── mod.rs
│   │   │   ├── service.rs
│   │   │   ├── storage.rs
│   │   │   └── models.rs
│   │   └── api/
│   │       ├── mod.rs
│   │       ├── device_keys.rs
│   │       ├── cross_signing.rs
│   │       ├── megolm.rs
│   │       ├── backup.rs
│   │       └── signature.rs
│   └── storage/
│       └── postgres/
│           └── migrations/
│               ├── 001_create_device_keys.sql
│               ├── 002_create_cross_signing_keys.sql
│               ├── 003_create_megolm_sessions.sql
│               ├── 004_create_key_backups.sql
│               └── 005_create_event_signatures.sql
└── tests/
    └── e2ee/
        ├── device_keys_test.rs
        ├── cross_signing_test.rs
        ├── megolm_test.rs
        ├── backup_test.rs
        └── integration_test.rs
```

### 2.2 依赖配置

```toml
[package]
name = "synapse-rust"
version = "0.1.0"
edition = "2021"

[dependencies]
# 异步运行时
tokio = { version = "1.35", features = ["full"] }

# Web 框架
axum = "0.7"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# 数据库
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "chrono", "uuid", "json"] }
deadpool-postgres = "0.12"

# 缓存
moka = { version = "0.12", features = ["future"] }
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }

# 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 加密库
ed25519-dalek = "2.0"
x25519-dalek = "2.0"
aes-gcm = "0.10"
argon2 = "0.5"
rand = "0.8"
zeroize = { version = "1.7", features = ["zeroize_derive"] }
secrecy = "0.8"

# 错误处理
thiserror = "1.0"
anyhow = "1.0"

# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 工具
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.6", features = ["v4", "serde"] }
base64 = "0.21"

[dev-dependencies]
tokio-test = "0.4"
criterion = "0.5"
```

### 2.3 模块导出

```rust
// src/e2ee/mod.rs
pub mod crypto;
pub mod device_keys;
pub mod cross_signing;
pub mod megolm;
pub mod backup;
pub mod signature;
pub mod api;

pub use crypto::*;
pub use device_keys::*;
pub use cross_signing::*;
pub use megolm::*;
pub use backup::*;
pub use signature::*;

// src/lib.rs
pub mod e2ee;

pub use e2ee::*;
```

---

## 三、加密库集成

### 3.1 Ed25519 签名实现

```rust
// src/e2ee/crypto/ed25519.rs
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ed25519PublicKey {
    bytes: [u8; 32],
}

impl Ed25519PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
    
    pub fn to_base64(&self) -> String {
        base64::encode(&self.bytes)
    }
    
    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = base64::decode(s)
            .map_err(|_| CryptoError::InvalidBase64)?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self::from_bytes(array))
    }
}

#[derive(Debug, Zeroize)]
pub struct Ed25519SecretKey {
    bytes: [u8; 32],
}

impl Ed25519SecretKey {
    pub fn generate() -> Self {
        let keypair = Keypair::generate(&mut OsRng);
        Self {
            bytes: keypair.secret.to_bytes(),
        }
    }
    
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Debug)]
pub struct Ed25519KeyPair {
    public: Ed25519PublicKey,
    secret: Ed25519SecretKey,
}

impl Ed25519KeyPair {
    pub fn generate() -> Self {
        let keypair = Keypair::generate(&mut OsRng);
        Self {
            public: Ed25519PublicKey::from_bytes(keypair.public.to_bytes()),
            secret: Ed25519SecretKey::from_bytes(keypair.secret.to_bytes()),
        }
    }
    
    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public
    }
    
    pub fn sign(&self, message: &[u8]) -> Signature {
        let secret = SecretKey::from_bytes(self.secret.as_bytes()).unwrap();
        let public = PublicKey::from_bytes(self.public.as_bytes()).unwrap();
        let keypair = Keypair { secret, public };
        keypair.sign(message)
    }
    
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), CryptoError> {
        let public = PublicKey::from_bytes(self.public.as_bytes()).unwrap();
        public.verify(message, signature)
            .map_err(|_| CryptoError::SignatureVerificationFailed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Invalid base64 encoding")]
    InvalidBase64,
    
    #[error("Invalid key length")]
    InvalidKeyLength,
    
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Decryption error: {0}")]
    DecryptionError(String),
}
```

### 3.2 X25519 密钥交换实现

```rust
// src/e2ee/crypto/x25519.rs
use x25519_dalek::{PublicKey, StaticSecret};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X25519PublicKey {
    bytes: [u8; 32],
}

impl X25519PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
    
    pub fn to_base64(&self) -> String {
        base64::encode(&self.bytes)
    }
    
    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = base64::decode(s)
            .map_err(|_| CryptoError::InvalidBase64)?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength);
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self::from_bytes(array))
    }
}

#[derive(Debug, Zeroize)]
pub struct X25519SecretKey {
    bytes: [u8; 32],
}

impl X25519SecretKey {
    pub fn generate() -> Self {
        let secret = StaticSecret::new(OsRng);
        Self {
            bytes: secret.to_bytes(),
        }
    }
    
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Debug)]
pub struct X25519KeyPair {
    public: X25519PublicKey,
    secret: X25519SecretKey,
}

impl X25519KeyPair {
    pub fn generate() -> Self {
        let secret = StaticSecret::new(OsRng);
        let public = PublicKey::from(&secret);
        Self {
            public: X25519PublicKey::from_bytes(public.to_bytes()),
            secret: X25519SecretKey::from_bytes(secret.to_bytes()),
        }
    }
    
    pub fn public_key(&self) -> &X25519PublicKey {
        &self.public
    }
    
    pub fn diffie_hellman(&self, other_public: &X25519PublicKey) -> [u8; 32] {
        let secret = StaticSecret::from(self.secret.as_bytes());
        let public = PublicKey::from(other_public.as_bytes());
        secret.diffie_hellman(&public).to_bytes()
    }
}

use super::ed25519::CryptoError;
```

### 3.3 AES-256-GCM 加密实现

```rust
// src/e2ee/crypto/aes.rs
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, NewAead}};
use rand::Rng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aes256GcmKey {
    bytes: [u8; 32],
}

impl Aes256GcmKey {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }
    
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aes256GcmNonce {
    bytes: [u8; 12],
}

impl Aes256GcmNonce {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 12];
        rand::thread_rng().fill(&mut bytes);
        Self { bytes }
    }
    
    pub fn from_bytes(bytes: [u8; 12]) -> Self {
        Self { bytes }
    }
    
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aes256GcmCiphertext {
    nonce: Aes256GcmNonce,
    ciphertext: Vec<u8>,
}

impl Aes256GcmCiphertext {
    pub fn new(nonce: Aes256GcmNonce, ciphertext: Vec<u8>) -> Self {
        Self { nonce, ciphertext }
    }
    
    pub fn nonce(&self) -> &Aes256GcmNonce {
        &self.nonce
    }
    
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

pub struct Aes256GcmCipher;

impl Aes256GcmCipher {
    pub fn encrypt(key: &Aes256GcmKey, plaintext: &[u8]) -> Result<Aes256GcmCiphertext, CryptoError> {
        let nonce = Aes256GcmNonce::generate();
        let cipher_key = Key::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(nonce.as_bytes());
        
        let ciphertext = cipher
            .encrypt(nonce_bytes, plaintext)
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;
        
        Ok(Aes256GcmCiphertext::new(nonce, ciphertext))
    }
    
    pub fn decrypt(key: &Aes256GcmKey, encrypted: &Aes256GcmCiphertext) -> Result<Vec<u8>, CryptoError> {
        let cipher_key = Key::from_slice(key.as_bytes());
        let cipher = Aes256Gcm::new(cipher_key);
        let nonce_bytes = Nonce::from_slice(encrypted.nonce().as_bytes());
        
        let plaintext = cipher
            .decrypt(nonce_bytes, encrypted.ciphertext().as_ref())
            .map_err(|e| CryptoError::DecryptionError(e.to_string()))?;
        
        Ok(plaintext)
    }
}

use super::ed25519::CryptoError;
```

### 3.4 Argon2 密钥派生实现

```rust
// src/e2ee/crypto/argon2.rs
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argon2Params {
    pub t_cost: u32,
    pub m_cost: u32,
    pub p_cost: u32,
    pub output_len: usize,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            t_cost: 3,
            m_cost: 65536,
            p_cost: 4,
            output_len: 32,
        }
    }
}

pub struct Argon2Kdf {
    algorithm: Argon2<'static>,
    params: Argon2Params,
}

impl Argon2Kdf {
    pub fn new(params: Argon2Params) -> Result<Self, CryptoError> {
        let algorithm = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(params.m_cost, params.t_cost, params.p_cost, None)
                .map_err(|e| CryptoError::HashError(e.to_string()))?,
        );
        Ok(Self { algorithm, params })
    }
    
    pub fn hash_password(&self, password: &str) -> Result<String, CryptoError> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self.algorithm
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| CryptoError::HashError(e.to_string()))?;
        Ok(password_hash.to_string())
    }
    
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, CryptoError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| CryptoError::HashError(e.to_string()))?;
        Ok(self.algorithm.verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }
    
    pub fn derive_key(&self, password: &str, salt: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut output = vec![0u8; self.params.output_len];
        self.algorithm.hash_password_into(
            self.algorithm.params(),
            password.as_bytes(),
            salt,
            &mut output
        ).map_err(|e| CryptoError::HashError(e.to_string()))?;
        Ok(output)
    }
}

use super::ed25519::CryptoError;
```

---

## 四、设备密钥管理实现

### 4.1 数据模型

```rust
// src/e2ee/device_keys/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKey {
    pub id: Uuid,
    pub user_id: String,
    pub device_id: String,
    pub display_name: Option<String>,
    pub algorithm: String,
    pub key_id: String,
    pub public_key: String,
    pub signatures: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeys {
    pub user_id: String,
    pub device_id: String,
    pub algorithms: Vec<String>,
    pub keys: serde_json::Value,
    pub signatures: serde_json::Value,
    pub unsigned: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryRequest {
    pub timeout: Option<u64>,
    pub device_keys: serde_json::Value,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyQueryResponse {
    pub device_keys: serde_json::Value,
    pub failures: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadRequest {
    pub device_keys: Option<DeviceKeys>,
    pub one_time_keys: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyUploadResponse {
    pub one_time_key_counts: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimRequest {
    pub timeout: Option<u64>,
    pub one_time_keys: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyClaimResponse {
    pub one_time_keys: serde_json::Value,
    pub failures: serde_json::Value,
}
```

### 4.2 存储层实现

```rust
// src/e2ee/device_keys/storage.rs
use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct DeviceKeyStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> DeviceKeyStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_device_key(&self, key: &DeviceKey) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO device_keys (id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (user_id, device_id, key_id) DO UPDATE
            SET display_name = EXCLUDED.display_name,
                public_key = EXCLUDED.public_key,
                signatures = EXCLUDED.signatures,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(key.id)
        .bind(&key.user_id)
        .bind(&key.device_id)
        .bind(&key.display_name)
        .bind(&key.algorithm)
        .bind(&key.key_id)
        .bind(&key.public_key)
        .bind(&key.signatures)
        .bind(key.created_at)
        .bind(key.updated_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_device_key(&self, user_id: &str, device_id: &str, key_id: &str) -> Result<Option<DeviceKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND key_id = $3
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(key_id)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| DeviceKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get("display_name"),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
    
    pub async fn get_device_keys(&self, user_id: &str, device_ids: &[String]) -> Result<Vec<DeviceKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, device_id, display_name, algorithm, key_id, public_key, signatures, created_at, updated_at
            FROM device_keys
            WHERE user_id = $1 AND device_id = ANY($2)
            "#
        )
        .bind(user_id)
        .bind(device_ids)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| DeviceKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            display_name: row.get("display_name"),
            algorithm: row.get("algorithm"),
            key_id: row.get("key_id"),
            public_key: row.get("public_key"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }).collect())
    }
    
    pub async fn delete_device_key(&self, user_id: &str, device_id: &str, key_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND key_id = $3
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .bind(key_id)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_one_time_keys_count(&self, user_id: &str, device_id: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM device_keys
            WHERE user_id = $1 AND device_id = $2 AND algorithm LIKE 'signed_curve25519%'
            "#
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_one(self.pool)
        .await?;
        
        Ok(row.get("count"))
    }
}
```

### 4.3 服务层实现

```rust
// src/e2ee/device_keys/service.rs
use super::models::*;
use super::storage::DeviceKeyStorage;
use crate::cache::CacheManager;
use crate::crypto::{Ed25519KeyPair, X25519KeyPair};
use std::sync::Arc;
use crate::error::ApiError;

pub struct DeviceKeyService {
    storage: DeviceKeyStorage<'static>,
    cache: Arc<CacheManager>,
}

impl DeviceKeyService {
    pub fn new(storage: DeviceKeyStorage<'static>, cache: Arc<CacheManager>) -> Self {
        Self { storage, cache }
    }
    
    pub async fn query_keys(&self, request: KeyQueryRequest) -> Result<KeyQueryResponse, ApiError> {
        let mut device_keys = serde_json::Map::new();
        let failures = serde_json::Map::new();
        
        if let Some(query_map) = request.device_keys.as_object() {
            for (user_id, device_ids) in query_map {
                let device_ids: Vec<String> = if let Some(arr) = device_ids.as_array() {
                    arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                } else {
                    vec!["*".to_string()]
                };
                
                let keys = if device_ids.contains(&"*".to_string()) {
                    // 查询所有设备
                    self.storage.get_all_device_keys(user_id).await?
                } else {
                    // 查询指定设备
                    self.storage.get_device_keys(user_id, &device_ids).await?
                };
                
                let mut user_keys = serde_json::Map::new();
                for key in keys {
                    let device_key = serde_json::json!({
                        "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                        "device_id": key.device_id,
                        "keys": {
                            format!("curve25519:{}", key.key_id): key.public_key,
                            format!("ed25519:{}", key.key_id): key.public_key,
                        },
                        "signatures": key.signatures,
                        "user_id": key.user_id,
                    });
                    user_keys.insert(key.device_id, device_key);
                }
                
                device_keys.insert(user_id.clone(), serde_json::Value::Object(user_keys));
            }
        }
        
        Ok(KeyQueryResponse {
            device_keys: serde_json::Value::Object(device_keys),
            failures: serde_json::Value::Object(failures),
        })
    }
    
    pub async fn upload_keys(&self, request: KeyUploadRequest) -> Result<KeyUploadResponse, ApiError> {
        let mut one_time_key_counts = serde_json::Map::new();
        
        if let Some(device_keys) = request.device_keys {
            let user_id = device_keys.user_id.clone();
            let device_id = device_keys.device_id.clone();
            
            // 存储设备密钥
            for (key_id, public_key) in device_keys.keys.as_object().unwrap() {
                let key = DeviceKey {
                    id: uuid::Uuid::new_v4(),
                    user_id: user_id.clone(),
                    device_id: device_id.clone(),
                    display_name: None,
                    algorithm: if key_id.contains("curve25519") {
                        "curve25519".to_string()
                    } else {
                        "ed25519".to_string()
                    },
                    key_id: key_id.clone(),
                    public_key: public_key.as_str().unwrap().to_string(),
                    signatures: device_keys.signatures.clone(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                
                self.storage.create_device_key(&key).await?;
                
                // 更新缓存
                let cache_key = format!("device_keys:{}:{}", user_id, device_id);
                self.cache.set(&cache_key, &key, 300).await;
            }
        }
        
        if let Some(one_time_keys) = request.one_time_keys {
            // 存储一次性密钥
            for (user_id, keys) in one_time_keys.as_object().unwrap() {
                for (device_id, device_keys) in keys.as_object().unwrap() {
                    for (key_id, key_data) in device_keys.as_object().unwrap() {
                        let key = DeviceKey {
                            id: uuid::Uuid::new_v4(),
                            user_id: user_id.clone(),
                            device_id: device_id.clone(),
                            display_name: None,
                            algorithm: "signed_curve25519".to_string(),
                            key_id: key_id.clone(),
                            public_key: key_data["key"].as_str().unwrap().to_string(),
                            signatures: key_data["signatures"].clone(),
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                        };
                        
                        self.storage.create_device_key(&key).await?;
                    }
                    
                    // 获取一次性密钥数量
                    let count = self.storage.get_one_time_keys_count(user_id, device_id).await?;
                    one_time_key_counts.insert("signed_curve25519".to_string(), serde_json::Value::Number(count.into()));
                }
            }
        }
        
        Ok(KeyUploadResponse {
            one_time_key_counts: serde_json::Value::Object(one_time_key_counts),
        })
    }
    
    pub async fn claim_keys(&self, request: KeyClaimRequest) -> Result<KeyClaimResponse, ApiError> {
        let mut one_time_keys = serde_json::Map::new();
        let failures = serde_json::Map::new();
        
        if let Some(claim_map) = request.one_time_keys.as_object() {
            for (user_id, device_keys) in claim_map {
                let mut user_keys = serde_json::Map::new();
                
                for (device_id, algorithm) in device_keys.as_object().unwrap() {
                    // 获取一次性密钥
                    let key = self.storage.claim_one_time_key(user_id, device_id, algorithm.as_str().unwrap()).await?;
                    
                    if let Some(key) = key {
                        let key_data = serde_json::json!({
                            "key": key.public_key,
                            "signatures": key.signatures,
                        });
                        user_keys.insert(device_id.clone(), serde_json::json!({
                            format!("{}:{}", algorithm.as_str().unwrap(), key.key_id): key_data
                        }));
                    }
                }
                
                one_time_keys.insert(user_id.clone(), serde_json::Value::Object(user_keys));
            }
        }
        
        Ok(KeyClaimResponse {
            one_time_keys: serde_json::Value::Object(one_time_keys),
            failures: serde_json::Value::Object(failures),
        })
    }
    
    pub async fn delete_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        // 删除设备密钥
        self.storage.delete_device_keys(user_id, device_id).await?;
        
        // 清除缓存
        let cache_key = format!("device_keys:{}:{}", user_id, device_id);
        self.cache.delete(&cache_key).await;
        
        Ok(())
    }
}
```

---

## 五、跨签名密钥管理实现

### 5.1 数据模型

```rust
// src/e2ee/cross_signing/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningKey {
    pub id: Uuid,
    pub user_id: String,
    pub key_type: String, // master, self_signing, user_signing
    pub public_key: String,
    pub usage: Vec<String>,
    pub signatures: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningKeys {
    pub user_id: String,
    pub master_key: String,
    pub self_signing_key: String,
    pub user_signing_key: String,
    pub self_signing_signature: String,
    pub user_signing_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSigningUpload {
    pub master_key: serde_json::Value,
    pub self_signing_key: serde_json::Value,
    pub user_signing_key: serde_json::Value,
}
```

### 5.2 存储层实现

```rust
// src/e2ee/cross_signing/storage.rs
use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct CrossSigningStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> CrossSigningStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_cross_signing_key(&self, key: &CrossSigningKey) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO cross_signing_keys (id, user_id, key_type, public_key, usage, signatures, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, key_type) DO UPDATE
            SET public_key = EXCLUDED.public_key,
                usage = EXCLUDED.usage,
                signatures = EXCLUDED.signatures,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(key.id)
        .bind(&key.user_id)
        .bind(&key.key_type)
        .bind(&key.public_key)
        .bind(&key.usage)
        .bind(&key.signatures)
        .bind(key.created_at)
        .bind(key.updated_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_cross_signing_key(&self, user_id: &str, key_type: &str) -> Result<Option<CrossSigningKey>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, key_type, public_key, usage, signatures, created_at, updated_at
            FROM cross_signing_keys
            WHERE user_id = $1 AND key_type = $2
            "#
        )
        .bind(user_id)
        .bind(key_type)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| CrossSigningKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            key_type: row.get("key_type"),
            public_key: row.get("public_key"),
            usage: row.get("usage"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }))
    }
    
    pub async fn get_cross_signing_keys(&self, user_id: &str) -> Result<Vec<CrossSigningKey>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, user_id, key_type, public_key, usage, signatures, created_at, updated_at
            FROM cross_signing_keys
            WHERE user_id = $1
            "#
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| CrossSigningKey {
            id: row.get("id"),
            user_id: row.get("user_id"),
            key_type: row.get("key_type"),
            public_key: row.get("public_key"),
            usage: row.get("usage"),
            signatures: row.get("signatures"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }).collect())
    }
}
```

### 5.3 服务层实现

```rust
// src/e2ee/cross_signing/service.rs
use super::models::*;
use super::storage::CrossSigningStorage;
use crate::crypto::Ed25519KeyPair;
use std::sync::Arc;
use crate::error::ApiError;

pub struct CrossSigningService {
    storage: CrossSigningStorage<'static>,
    device_key_service: Arc<DeviceKeyService>,
}

impl CrossSigningService {
    pub fn new(storage: CrossSigningStorage<'static>, device_key_service: Arc<DeviceKeyService>) -> Self {
        Self { storage, device_key_service }
    }
    
    pub async fn upload_cross_signing_keys(&self, upload: CrossSigningUpload) -> Result<(), ApiError> {
        let user_id = upload.master_key["user_id"].as_str().unwrap();
        
        // 存储主密钥
        let master_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "master".to_string(),
            public_key: upload.master_key["keys"]["ed25519:MASTER"].as_str().unwrap().to_string(),
            usage: upload.master_key["usage"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            signatures: upload.master_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&master_key).await?;
        
        // 存储自签名密钥
        let self_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "self_signing".to_string(),
            public_key: upload.self_signing_key["keys"]["ed25519:SELF_SIGNING"].as_str().unwrap().to_string(),
            usage: upload.self_signing_key["usage"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            signatures: upload.self_signing_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&self_signing_key).await?;
        
        // 存储用户签名密钥
        let user_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "user_signing".to_string(),
            public_key: upload.user_signing_key["keys"]["ed25519:USER_SIGNING"].as_str().unwrap().to_string(),
            usage: upload.user_signing_key["usage"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            signatures: upload.user_signing_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&user_signing_key).await?;
        
        Ok(())
    }
    
    pub async fn get_cross_signing_keys(&self, user_id: &str) -> Result<CrossSigningKeys, ApiError> {
        let keys = self.storage.get_cross_signing_keys(user_id).await?;
        
        let master_key = keys.iter().find(|k| k.key_type == "master").unwrap();
        let self_signing_key = keys.iter().find(|k| k.key_type == "self_signing").unwrap();
        let user_signing_key = keys.iter().find(|k| k.key_type == "user_signing").unwrap();
        
        Ok(CrossSigningKeys {
            user_id: user_id.to_string(),
            master_key: master_key.public_key.clone(),
            self_signing_key: self_signing_key.public_key.clone(),
            user_signing_key: user_signing_key.public_key.clone(),
            self_signing_signature: String::new(),
            user_signing_signature: String::new(),
        })
    }
    
    pub async fn sign_device_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        // 获取自签名密钥
        let self_signing_key = self.storage.get_cross_signing_key(user_id, "self_signing").await?;
        if self_signing_key.is_none() {
            return Err(ApiError::NotFound("Self-signing key not found".to_string()));
        }
        
        // 获取设备密钥
        let device_keys = self.device_key_service.query_keys(KeyQueryRequest {
            device_keys: serde_json::json!({
                user_id: [device_id]
            }),
            ..Default::default()
        }).await?;
        
        // 签名设备密钥
        // ... 实现签名逻辑
        
        Ok(())
    }
    
    pub async fn verify_device_keys(&self, user_id: &str, device_id: &str) -> Result<bool, ApiError> {
        // 获取设备密钥
        let device_keys = self.device_key_service.query_keys(KeyQueryRequest {
            device_keys: serde_json::json!({
                user_id: [device_id]
            }),
            ..Default::default()
        }).await?;
        
        // 获取自签名密钥
        let self_signing_key = self.storage.get_cross_signing_key(user_id, "self_signing").await?;
        if self_signing_key.is_none() {
            return Ok(false);
        }
        
        // 验证签名
        // ... 实现验证逻辑
        
        Ok(true)
    }
}
```

---

## 六、Megolm 加密服务实现

### 6.1 数据模型

```rust
// src/e2ee/megolm/models.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MegolmSession {
    pub id: Uuid,
    pub session_id: String,
    pub room_id: String,
    pub sender_key: String,
    pub session_key: String, // 加密存储
    pub algorithm: String,
    pub message_index: i64,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEvent {
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub content: serde_json::Value,
    pub algorithm: String,
    pub session_id: String,
    pub ciphertext: String,
    pub device_id: String,
}
```

### 6.2 存储层实现

```rust
// src/e2ee/megolm/storage.rs
use super::models::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;
use crate::error::ApiError;

pub struct MegolmSessionStorage<'a> {
    pool: &'a PgPool,
}

impl<'a> MegolmSessionStorage<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            INSERT INTO megolm_sessions (id, session_id, room_id, sender_key, session_key, algorithm, message_index, created_at, last_used_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(session.id)
        .bind(&session.session_id)
        .bind(&session.room_id)
        .bind(&session.sender_key)
        .bind(&session.session_key)
        .bind(&session.algorithm)
        .bind(session.message_index)
        .bind(session.created_at)
        .bind(session.last_used_at)
        .bind(session.expires_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_session(&self, session_id: &str) -> Result<Option<MegolmSession>, ApiError> {
        let row = sqlx::query(
            r#"
            SELECT id, session_id, room_id, sender_key, session_key, algorithm, message_index, created_at, last_used_at, expires_at
            FROM megolm_sessions
            WHERE session_id = $1
            "#
        )
        .bind(session_id)
        .fetch_optional(self.pool)
        .await?;
        
        Ok(row.map(|row| MegolmSession {
            id: row.get("id"),
            session_id: row.get("session_id"),
            room_id: row.get("room_id"),
            sender_key: row.get("sender_key"),
            session_key: row.get("session_key"),
            algorithm: row.get("algorithm"),
            message_index: row.get("message_index"),
            created_at: row.get("created_at"),
            last_used_at: row.get("last_used_at"),
            expires_at: row.get("expires_at"),
        }))
    }
    
    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, room_id, sender_key, session_key, algorithm, message_index, created_at, last_used_at, expires_at
            FROM megolm_sessions
            WHERE room_id = $1
            "#
        )
        .bind(room_id)
        .fetch_all(self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| MegolmSession {
            id: row.get("id"),
            session_id: row.get("session_id"),
            room_id: row.get("room_id"),
            sender_key: row.get("sender_key"),
            session_key: row.get("session_key"),
            algorithm: row.get("algorithm"),
            message_index: row.get("message_index"),
            created_at: row.get("created_at"),
            last_used_at: row.get("last_used_at"),
            expires_at: row.get("expires_at"),
        }).collect())
    }
    
    pub async fn update_session(&self, session: &MegolmSession) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE megolm_sessions
            SET session_key = $2, message_index = $3, last_used_at = $4, expires_at = $5
            WHERE session_id = $1
            "#
        )
        .bind(&session.session_id)
        .bind(&session.session_key)
        .bind(session.message_index)
        .bind(session.last_used_at)
        .bind(session.expires_at)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            DELETE FROM megolm_sessions
            WHERE session_id = $1
            "#
        )
        .bind(session_id)
        .execute(self.pool)
        .await?;
        
        Ok(())
    }
}
```

### 6.3 服务层实现

```rust
// src/e2ee/megolm/service.rs
use super::models::*;
use super::storage::MegolmSessionStorage;
use crate::crypto::Aes256GcmCipher;
use crate::cache::CacheManager;
use std::sync::Arc;
use crate::error::ApiError;

pub struct MegolmService {
    storage: MegolmSessionStorage<'static>,
    cache: Arc<CacheManager>,
    encryption_key: [u8; 32],
}

impl MegolmService {
    pub fn new(storage: MegolmSessionStorage<'static>, cache: Arc<CacheManager>, encryption_key: [u8; 32]) -> Self {
        Self { storage, cache, encryption_key }
    }
    
    pub async fn create_session(&self, room_id: &str, sender_key: &str) -> Result<MegolmSession, ApiError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        
        // 生成会话密钥
        let session_key = Aes256GcmCipher::generate_key();
        let encrypted_key = self.encrypt_session_key(&session_key)?;
        
        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            session_key: encrypted_key,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_at: Utc::now(),
            last_used_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(7)),
        };
        
        self.storage.create_session(&session).await?;
        
        // 缓存会话
        let cache_key = format!("megolm_session:{}", session_id);
        self.cache.set(&cache_key, &session, 600).await;
        
        Ok(session)
    }
    
    pub async fn load_session(&self, session_id: &str) -> Result<MegolmSession, ApiError> {
        // 先查缓存
        let cache_key = format!("megolm_session:{}", session_id);
        if let Some(session) = self.cache.get::<MegolmSession>(&cache_key).await {
            return Ok(session);
        }
        
        // 查数据库
        let session = self.storage.get_session(session_id).await?
            .ok_or_else(|| ApiError::NotFound("Session not found".to_string()))?;
        
        // 更新缓存
        self.cache.set(&cache_key, &session, 600).await;
        
        Ok(session)
    }
    
    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;
        
        let cipher = Aes256GcmCipher::new(&session_key);
        let encrypted = cipher.encrypt(plaintext)?;
        
        // 更新消息索引
        let mut updated_session = session.clone();
        updated_session.message_index += 1;
        updated_session.last_used_at = Utc::now();
        self.storage.update_session(&updated_session).await?;
        
        Ok(encrypted)
    }
    
    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;
        
        let cipher = Aes256GcmCipher::new(&session_key);
        let decrypted = cipher.decrypt(ciphertext)?;
        
        Ok(decrypted)
    }
    
    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;
        
        // 删除旧会话
        self.storage.delete_session(session_id).await?;
        
        // 创建新会话
        self.create_session(&session.room_id, &session.sender_key).await?;
        
        Ok(())
    }
    
    pub async fn share_session(&self, session_id: &str, user_ids: &[String]) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;
        
        // 分发会话密钥给所有用户
        for user_id in user_ids {
            // 使用用户的设备公钥加密会话密钥
            // ... 实现密钥分发逻辑
        }
        
        Ok(())
    }
    
    fn encrypt_session_key(&self, key: &[u8; 32]) -> Result<String, ApiError> {
        let cipher = Aes256GcmCipher::new(&Aes256GcmCipher::from_bytes(self.encryption_key));
        let encrypted = cipher.encrypt(key)?;
        Ok(base64::encode(&encrypted))
    }
    
    fn decrypt_session_key(&self, encrypted: &str) -> Result<[u8; 32], ApiError> {
        let encrypted_bytes = base64::decode(encrypted)
            .map_err(|_| ApiError::DecryptionError("Invalid base64".to_string()))?;
        let cipher = Aes256GcmCipher::new(&Aes256GcmCipher::from_bytes(self.encryption_key));
        let decrypted = cipher.decrypt(&encrypted_bytes)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&decrypted);
        Ok(key)
    }
}
```

---

## 七、API 端点实现

### 7.1 设备密钥 API

```rust
// src/e2ee/api/device_keys.rs
use axum::{
    extract::{State, Path},
    Json,
};
use super::super::device_keys::{DeviceKeyService, KeyQueryRequest, KeyUploadRequest, KeyClaimRequest};
use crate::error::ApiError;

pub async fn query_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyQueryRequest>,
) -> Result<Json<KeyQueryResponse>, ApiError> {
    let response = service.query_keys(request).await?;
    Ok(Json(response))
}

pub async fn upload_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyUploadRequest>,
) -> Result<Json<KeyUploadResponse>, ApiError> {
    let response = service.upload_keys(request).await?;
    Ok(Json(response))
}

pub async fn claim_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Json(request): Json<KeyClaimRequest>,
) -> Result<Json<KeyClaimResponse>, ApiError> {
    let response = service.claim_keys(request).await?;
    Ok(Json(response))
}

pub async fn delete_keys(
    State(service): State<Arc<DeviceKeyService>>,
    Path((user_id, device_id)): Path<(String, String)>,
) -> Result<Json<()>, ApiError> {
    service.delete_keys(&user_id, &device_id).await?;
    Ok(Json(()))
}
```

### 7.2 路由注册

```rust
// src/e2ee/api/mod.rs
use axum::{
    Router,
    routing::{get, post, delete},
};
use super::device_keys::*;
use super::cross_signing::*;
use super::megolm::*;
use super::backup::*;

pub fn e2ee_routes() -> Router {
    Router::new()
        // 设备密钥 API
        .route("/keys/query", post(query_keys))
        .route("/keys/upload", post(upload_keys))
        .route("/keys/claim", post(claim_keys))
        .route("/keys/:user_id/:device_id", delete(delete_keys))
        
        // 跨签名密钥 API
        .route("/keys/device_signing/upload", post(upload_cross_signing_keys))
        .route("/keys/signatures/upload", post(upload_signatures))
        
        // Megolm API
        .route("/rooms/:room_id/encryption", post(enable_encryption))
        .route("/rooms/:room_id/encryption", delete(disable_encryption))
        
        // 密钥备份 API
        .route("/room_keys/version", post(create_backup))
        .route("/room_keys/version", get(get_backup))
        .route("/room_keys/version/:version", delete(delete_backup))
        .route("/room_keys/keys/:room_id", put(upload_backup_keys))
        .route("/room_keys/keys/:room_id", get(download_backup_keys))
}
```

---

## 八、测试实现

### 8.1 单元测试

```rust
// tests/e2ee/device_keys_test.rs
use synapse_rust::e2ee::device_keys::*;
use synapse_rust::crypto::*;

#[tokio::test]
async fn test_device_key_generation() {
    let keypair = Ed25519KeyPair::generate();
    assert!(!keypair.public_key().as_bytes().is_empty());
    
    let message = b"test message";
    let signature = keypair.sign(message);
    assert!(keypair.verify(message, &signature).is_ok());
}

#[tokio::test]
async fn test_device_key_storage() {
    let pool = create_test_pool().await;
    let storage = DeviceKeyStorage::new(&pool);
    
    let key = DeviceKey {
        id: uuid::Uuid::new_v4(),
        user_id: "@alice:example.com".to_string(),
        device_id: "DEVICE1".to_string(),
        display_name: Some("Alice's Phone".to_string()),
        algorithm: "ed25519".to_string(),
        key_id: "ed25519:DEVICE1".to_string(),
        public_key: "base64_public_key".to_string(),
        signatures: serde_json::json!({}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    storage.create_device_key(&key).await.unwrap();
    
    let retrieved = storage.get_device_key("@alice:example.com", "DEVICE1", "ed25519:DEVICE1").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().device_id, "DEVICE1");
}

#[tokio::test]
async fn test_device_key_service() {
    let pool = create_test_pool().await;
    let cache = Arc::new(CacheManager::new());
    let storage = DeviceKeyStorage::new(&pool);
    let service = DeviceKeyService::new(storage, cache);
    
    let request = KeyUploadRequest {
        device_keys: Some(DeviceKeys {
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE1".to_string(),
            algorithms: vec!["m.olm.v1.curve25519-aes-sha2".to_string()],
            keys: serde_json::json!({
                "ed25519:DEVICE1": "base64_public_key",
                "curve25519:DEVICE1": "base64_public_key",
            }),
            signatures: serde_json::json!({}),
            unsigned: None,
        }),
        one_time_keys: None,
    };
    
    let response = service.upload_keys(request).await.unwrap();
    assert!(response.one_time_key_counts.is_object());
}
```

### 8.2 集成测试

```rust
// tests/e2ee/integration_test.rs
use synapse_rust::e2ee::*;
use synapse_rust::crypto::*;

#[tokio::test]
async fn test_e2ee_message_flow() {
    let pool = create_test_pool().await;
    let cache = Arc::new(CacheManager::new());
    
    // 创建用户
    let alice = create_test_user("alice").await;
    let bob = create_test_user("bob").await;
    
    // Alice 创建加密房间
    let room_id = create_test_room(&pool, &alice.user_id).await;
    
    // Alice 发送加密消息
    let megolm_service = create_megolm_service(&pool, cache.clone()).await;
    let session = megolm_service.create_session(&room_id, &alice.device_id).await.unwrap();
    
    let message = b"Hello, Bob!";
    let encrypted = megolm_service.encrypt(&session.session_id, message).await.unwrap();
    
    // Bob 接收并解密消息
    let decrypted = megolm_service.decrypt(&session.session_id, &encrypted).await.unwrap();
    assert_eq!(decrypted, message);
}
```

---

## 九、部署与配置

### 9.1 环境变量配置

```bash
# .env
DATABASE_URL=postgres://synapse:synapse@localhost:5432/synapse
REDIS_URL=redis://localhost:6379
E2EE_ENCRYPTION_KEY=your-256-bit-encryption-key
JWT_SECRET=your-jwt-secret
```

### 9.2 配置文件

```yaml
# config/e2ee.yaml
e2ee:
  enabled: true
  algorithms:
    - m.olm.v1.curve25519-aes-sha2
    - m.megolm.v1.aes-sha2
  cache:
    enabled: true
    local_ttl: 300
    redis_ttl: 600
  rate_limits:
    key_upload: 10
    key_query: 100
    encryption: 1000
```

### 9.3 数据库迁移

```bash
# 运行迁移
cargo install sqlx-cli
sqlx database create
sqlx migrate run

# 或使用 Docker
docker run --rm -v $(pwd)/migrations:/migrations \
  -e DATABASE_URL=postgres://synapse:synapse@localhost:5432/synapse \
  sqlx-cli sqlx migrate run
```

---

## 十、故障排查

### 10.1 常见问题

**问题 1：密钥查询失败**
```
Error: Database connection failed
```
解决方案：
- 检查数据库连接配置
- 验证数据库凭证
- 检查网络连接

**问题 2：加密/解密失败**
```
Error: Decryption failed: Invalid ciphertext
```
解决方案：
- 检查会话是否存在
- 验证密钥是否正确
- 检查算法是否匹配

**问题 3：签名验证失败**
```
Error: Signature verification failed
```
解决方案：
- 检查签名密钥是否正确
- 验证签名数据是否完整
- 检查签名算法是否匹配

### 10.2 调试工具

```rust
// 调试工具
pub struct E2EEDebugTool {
    device_key_service: Arc<DeviceKeyService>,
    megolm_service: Arc<MegolmService>,
}

impl E2EEDebugTool {
    pub async fn debug_device_keys(&self, user_id: &str) {
        let keys = self.device_key_service.query_keys(KeyQueryRequest {
            device_keys: serde_json::json!({ user_id: ["*"] }),
            ..Default::default()
        }).await;
        println!("Device keys: {:?}", keys);
    }
    
    pub async fn debug_megolm_session(&self, session_id: &str) {
        let session = self.megolm_service.load_session(session_id).await;
        println!("Session: {:?}", session);
    }
}
```

---

## 十一、性能优化

### 11.1 缓存优化

```rust
// 批量缓存预热
pub async fn warmup_cache(service: &DeviceKeyService, user_ids: &[String]) {
    for user_id in user_ids {
        let _ = service.query_keys(KeyQueryRequest {
            device_keys: serde_json::json!({ user_id: ["*"] }),
            ..Default::default()
        }).await;
    }
}
```

### 11.2 批量处理

```rust
// 批量加密
pub async fn batch_encrypt(
    service: &MegolmService,
    session_id: &str,
    messages: Vec<Vec<u8>>,
) -> Result<Vec<Vec<u8>>> {
    let futures: Vec<_> = messages
        .into_iter()
        .map(|msg| service.encrypt(session_id, &msg))
        .collect();
    
    let results = futures::future::join_all(futures).await;
    results.into_iter().collect()
}
```

---

## 十二、安全最佳实践

### 12.1 密钥安全

```rust
// 使用 secrecy 保护密钥
use secrecy::{Secret, ExposeSecret};

pub struct SecureKey {
    key: Secret<[u8; 32]>,
}

impl SecureKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key: Secret::new(key) }
    }
    
    pub fn expose(&self) -> &[u8; 32] {
        self.key.expose_secret()
    }
}
```

### 12.2 输入验证

```rust
// 验证用户输入
pub fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if !user_id.starts_with('@') || !user_id.contains(':') {
        return Err(ApiError::InvalidInput("Invalid user ID format".to_string()));
    }
    Ok(())
}
```

---

## 附录

### A. 完整代码示例

参见项目源码：`src/e2ee/`

### B. 测试用例

参见测试目录：`tests/e2ee/`

### C. 配置示例

参见配置文件：`config/e2ee.yaml`

---

**文档结束**
