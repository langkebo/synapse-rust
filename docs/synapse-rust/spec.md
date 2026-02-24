# Synapse Rust 项目优化规范

> **版本**：1.0.0  
> **创建日期**：2026-02-24  
> **参考文档**：[Matrix 规范](https://spec.matrix.org/)、[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[vodozemac 文档](https://crates.io/crates/vodozemac)

---

## 一、项目现状分析

### 1.1 已实现功能

| 模块 | 实现状态 | 说明 |
|------|---------|------|
| 基础 Olm 服务 | ✅ 已实现 | `src/e2ee/olm/service.rs` - 基础的 vodozemac 集成 |
| Megolm 群组加密 | ✅ 已实现 | `src/e2ee/megolm/` - AES-256-GCM 加密 |
| Worker 配置结构 | ✅ 已实现 | `src/common/config.rs` - WorkerConfig |
| Worker 管理器 | ✅ 已实现 | `src/worker/manager.rs` - WorkerManager |
| 复制协议 | ✅ 已实现 | `src/worker/protocol.rs` - ReplicationCommand |
| Push 服务框架 | ✅ 已实现 | `src/services/push_notification_service.rs` |
| 设备密钥管理 | ✅ 已实现 | `src/e2ee/device_keys/` |
| 交叉签名 | ✅ 已实现 | `src/e2ee/cross_signing/` |
| 密钥备份 | ✅ 已实现 | `src/e2ee/backup/` |
| To-Device 消息 | ✅ 已实现 | `src/e2ee/to_device/` |

### 1.2 待优化功能

| 模块 | 当前状态 | 优化需求 |
|------|---------|---------|
| Olm 会话管理 | 基础实现 | 缺少会话持久化、会话恢复、批量操作 |
| Worker 通信 | TCP 协议定义 | 缺少 Redis Pub/Sub 消息总线实现 |
| Push 推送 | 框架存在 | 缺少 FCM/APNs/WebPush 实际集成 |
| 会话存储 | 内存存储 | 需要数据库持久化 |

---

## 二、优化方案详情

### 2.1 E2EE 双棘轮算法优化

#### 2.1.1 Matrix 规范要求

根据 Matrix 规范，Olm 协议必须实现以下功能：

1. **OlmAccount 管理**
   - 生成身份密钥对（Curve25519）
   - 生成一次性密钥（One-Time Keys）
   - 生成回退密钥（Fallback Key）
   - 签名消息

2. **OlmSession 管理**
   - 创建出站会话（Outbound Session）
   - 创建入站会话（Inbound Session）
   - 消息加密/解密
   - 会话棘轮（Ratchet）

3. **会话持久化**
   - 会话状态序列化
   - 会话恢复
   - 会话过期管理

#### 2.1.2 当前实现差距

```rust
// 当前实现 - src/e2ee/olm/service.rs
pub struct OlmService {
    account: vodozemac::olm::Account,
    cache: Arc<CacheManager>,
}

// 缺少:
// 1. 会话存储 (sessions: HashMap<String, OlmSession>)
// 2. 数据库持久化层
// 3. 会话恢复机制
// 4. 与 to_device 的集成
```

#### 2.1.3 优化方案

**新增文件结构**：

```
src/e2ee/olm/
├── mod.rs              # 模块导出
├── models.rs           # 数据模型 (已存在，需扩展)
├── service.rs          # Olm 服务 (已存在，需扩展)
├── storage.rs          # 新增：数据库持久化
├── session.rs          # 新增：会话管理
└── integration.rs      # 新增：与 to_device 集成
```

**数据模型扩展**：

```rust
// models.rs 扩展
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmSessionData {
    pub session_id: String,
    pub sender_key: String,
    pub receiver_key: String,
    pub created_at: i64,
    pub last_used_at: i64,
    pub message_index: u32,
    pub serialized_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlmAccountData {
    pub user_id: String,
    pub device_id: String,
    pub identity_key: String,
    pub serialized_account: String,
    pub one_time_keys_published: bool,
    pub fallback_key_published: bool,
}
```

**存储层实现**：

```rust
// storage.rs
pub struct OlmStorage {
    pool: PgPool,
}

impl OlmStorage {
    pub async fn save_account(&self, account: &OlmAccountData) -> Result<()>;
    pub async fn load_account(&self, user_id: &str, device_id: &str) -> Result<Option<OlmAccountData>>;
    pub async fn save_session(&self, session: &OlmSessionData) -> Result<()>;
    pub async fn load_sessions(&self, user_id: &str, device_id: &str) -> Result<Vec<OlmSessionData>>;
    pub async fn delete_session(&self, session_id: &str) -> Result<()>;
}
```

**数据库迁移**：

```sql
-- migrations/YYYYMMDDHHMMSS_create_olm_tables.sql

CREATE TABLE IF NOT EXISTS olm_accounts (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    identity_key VARCHAR(255) NOT NULL,
    serialized_account TEXT NOT NULL,
    one_time_keys_published BOOLEAN DEFAULT FALSE,
    fallback_key_published BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, device_id)
);

CREATE TABLE IF NOT EXISTS olm_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    sender_key VARCHAR(255) NOT NULL,
    receiver_key VARCHAR(255) NOT NULL,
    serialized_state TEXT NOT NULL,
    message_index INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    expires_ts BIGINT
);

CREATE INDEX idx_olm_sessions_user_device ON olm_sessions(user_id, device_id);
CREATE INDEX idx_olm_sessions_sender_key ON olm_sessions(sender_key);
```

---

### 2.2 Workers 架构优化

#### 2.2.1 Matrix 规范要求

Synapse Workers 架构需要支持：

1. **多实例部署**
   - Master Worker（主节点）
   - Frontend Worker（前端处理）
   - Federation Sender（联邦发送）
   - Event Persister（事件持久化）
   - Media Repository（媒体存储）
   - Pusher（推送通知）

2. **实例间通信**
   - TCP 复制协议
   - Redis Pub/Sub 消息总线
   - 流位置同步

3. **流写入器分配**
   - Events Stream
   - Typing Stream
   - To-Device Stream
   - Account Data Stream
   - Receipts Stream
   - Presence Stream

#### 2.2.2 当前实现差距

```rust
// 当前实现 - src/worker/manager.rs
pub struct WorkerManager {
    storage: Arc<WorkerStorage>,
    server_name: String,
    local_worker_id: Option<String>,
    connections: Arc<RwLock<HashMap<String, ReplicationConnection>>>,
    protocol: ReplicationProtocol,
}

// 缺少:
// 1. Redis Pub/Sub 消息总线
// 2. 流写入器分配逻辑
// 3. 负载均衡策略
// 4. 健康检查机制
```

#### 2.2.3 优化方案

**新增文件结构**：

```
src/worker/
├── mod.rs              # 模块导出 (已存在)
├── types.rs            # 类型定义 (已存在)
├── storage.rs          # 存储层 (已存在)
├── protocol.rs         # 复制协议 (已存在)
├── manager.rs          # Worker 管理器 (已存在，需扩展)
├── tcp.rs              # TCP 连接 (已存在)
├── bus.rs              # 新增：Redis Pub/Sub 消息总线
├── stream.rs           # 新增：流写入器管理
├── load_balancer.rs    # 新增：负载均衡
└── health.rs           # 新增：健康检查
```

**Redis Pub/Sub 消息总线**：

```rust
// bus.rs
pub struct WorkerBus {
    redis: Arc<RedisConnectionManager>,
    server_name: String,
    instance_name: String,
}

impl WorkerBus {
    pub async fn publish(&self, channel: &str, message: &[u8]) -> Result<()>;
    pub async fn subscribe(&self, channels: &[&str]) -> Result<Receiver<Vec<u8>>>;
    pub async fn broadcast_command(&self, command: &ReplicationCommand) -> Result<()>;
    pub async fn send_to_worker(&self, worker_id: &str, command: &ReplicationCommand) -> Result<()>;
}
```

**流写入器管理**：

```rust
// stream.rs
pub struct StreamWriterManager {
    config: StreamWriters,
    bus: Arc<WorkerBus>,
}

impl StreamWriterManager {
    pub fn get_writer(&self, stream_name: &str) -> Option<&str>;
    pub fn is_local_writer(&self, stream_name: &str) -> bool;
    pub async fn forward_to_writer(&self, stream_name: &str, data: &[u8]) -> Result<()>;
}
```

**负载均衡**：

```rust
// load_balancer.rs
pub struct WorkerLoadBalancer {
    workers: Arc<RwLock<HashMap<String, WorkerInfo>>>,
    strategy: LoadBalanceStrategy,
}

pub enum LoadBalanceStrategy {
    RoundRobin,
    LeastConnections,
    WeightedRoundRobin,
}

impl WorkerLoadBalancer {
    pub fn select_worker(&self, task_type: &str) -> Option<String>;
    pub fn update_worker_load(&self, worker_id: &str, load: WorkerLoadStats);
}
```

---

### 2.3 Push 通知优化

#### 2.3.1 Matrix 规范要求

Matrix Push 网关规范要求：

1. **Push Gateway 协议**
   - HTTP POST 到推送网关
   - 通知格式：`{ event_id, room_id, type, sender, ... }`
   - 推送密钥（Push Key）管理

2. **Push 规则**
   - Override 规则
   - Content 规则
   - Room 规则
   - Sender 规则
   - Underride 规则

3. **多平台支持**
   - FCM (Firebase Cloud Messaging)
   - APNs (Apple Push Notification Service)
   - Web Push (VAPID)

#### 2.3.2 当前实现差距

```rust
// 当前实现 - src/services/push_notification_service.rs
async fn send_fcm(&self, token: &str, _payload: &NotificationPayload) -> Result<...> {
    // 当前只是占位符实现
    info!("Sending FCM notification to token: {}...", &token[..20]);
    Ok((true, None, Some("FCM accepted".to_string())))
}

// 缺少:
// 1. 真正的 HTTP 请求到 FCM/APNs/Web Push
// 2. 错误重试机制
// 3. 批量发送优化
// 4. 推送网关协议实现
```

#### 2.3.3 优化方案

**新增文件结构**：

```
src/services/push/
├── mod.rs                    # 模块导出
├── service.rs                # 推送服务主逻辑
├── providers/
│   ├── mod.rs
│   ├── fcm.rs               # FCM 实现
│   ├── apns.rs              # APNs 实现
│   └── webpush.rs           # Web Push 实现
├── gateway.rs               # Push Gateway 协议
├── rules.rs                 # Push 规则处理
└── queue.rs                 # 推送队列管理
```

**FCM Provider 实现**：

```rust
// providers/fcm.rs
pub struct FcmProvider {
    api_key: String,
    http_client: reqwest::Client,
    endpoint: String,
}

impl FcmProvider {
    pub async fn send(&self, token: &str, payload: &NotificationPayload) -> Result<PushResult>;
    pub async fn send_batch(&self, messages: Vec<FcmMessage>) -> Result<Vec<PushResult>>;
}

#[derive(Serialize)]
struct FcmMessage {
    to: String,
    notification: FcmNotification,
    data: Option<serde_json::Value>,
    priority: String,
}
```

**APNs Provider 实现**：

```rust
// providers/apns.rs
pub struct ApnsProvider {
    topic: String,
    endpoint: String,
    private_key: Option<String>,
    key_id: Option<String>,
    team_id: Option<String>,
    http_client: reqwest::Client,
}

impl ApnsProvider {
    pub async fn send(&self, token: &str, payload: &NotificationPayload) -> Result<PushResult>;
    fn generate_jwt(&self) -> Result<String>;
}

#[derive(Serialize)]
struct ApnsPayload {
    aps: ApnsAps,
}
```

**Web Push Provider 实现**：

```rust
// providers/webpush.rs
pub struct WebPushProvider {
    vapid_private_key: String,
    vapid_public_key: String,
    subject: String,
    http_client: reqwest::Client,
}

impl WebPushProvider {
    pub async fn send(&self, subscription: &WebPushSubscription, payload: &[u8]) -> Result<PushResult>;
    fn encrypt_payload(&self, payload: &[u8], subscription: &WebPushSubscription) -> Result<EncryptedPayload>;
    fn generate_vapid_jwt(&self, endpoint: &str) -> Result<String>;
}
```

**Push Gateway 协议**：

```rust
// gateway.rs
pub struct PushGateway {
    http_client: reqwest::Client,
}

impl PushGateway {
    pub async fn send_notification(
        &self,
        gateway_url: &str,
        notification: &PushNotification,
    ) -> Result<PushGatewayResponse>;
}

#[derive(Serialize)]
pub struct PushNotification {
    pub notification: NotificationContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<PushDevice>>,
}

#[derive(Serialize)]
pub struct NotificationContent {
    pub event_id: String,
    pub room_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub sender: String,
    pub counts: NotificationCounts,
}
```

---

## 三、API 端点规范

### 3.1 E2EE 相关端点

| 端点 | 方法 | 说明 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/keys/upload` | POST | 上传设备密钥 | ✅ 已实现 |
| `/_matrix/client/v3/keys/query` | POST | 查询设备密钥 | ✅ 已实现 |
| `/_matrix/client/v3/keys/claim` | POST | 声明一次性密钥 | ✅ 已实现 |
| `/_matrix/client/v3/keys/device_signing/upload` | POST | 上传交叉签名 | ✅ 已实现 |
| `/_matrix/client/v3/room_keys/version` | POST | 创建密钥备份版本 | ✅ 已实现 |
| `/_matrix/client/v3/sendToDevice/{eventType}/{txnId}` | PUT | 发送到设备消息 | ✅ 已实现 |

### 3.2 Worker 相关端点

| 端点 | 方法 | 说明 | 状态 |
|------|------|------|------|
| `/_synapse/replication/{command}` | * | 复制协议 | ✅ 已实现 |
| `/_synapse/worker/register` | POST | Worker 注册 | ✅ 已实现 |
| `/_synapse/worker/heartbeat` | POST | Worker 心跳 | ✅ 已实现 |

### 3.3 Push 相关端点

| 端点 | 方法 | 说明 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/pushers/set` | POST | 设置推送器 | ✅ 已实现 |
| `/_matrix/client/v3/pushers` | GET | 获取推送器列表 | ✅ 已实现 |
| `/_matrix/client/v3/pushrules` | GET | 获取推送规则 | ✅ 已实现 |
| `/_matrix/client/v3/pushrules/{scope}/{kind}/{ruleId}` | PUT | 设置推送规则 | ✅ 已实现 |
| `/_matrix/client/v3/notifications` | GET | 获取通知列表 | ✅ 已实现 |

---

## 四、数据库迁移规范

### 4.1 迁移文件命名

```
migrations/
├── 20260224000001_create_olm_tables.sql
├── 20260224000002_create_worker_bus_tables.sql
└── 20260224000003_create_push_provider_tables.sql
```

### 4.2 迁移内容

**Olm 表**：
- `olm_accounts` - Olm 账户存储
- `olm_sessions` - Olm 会话存储

**Worker Bus 表**：
- `worker_stream_positions` - 流位置跟踪
- `worker_messages` - 消息队列

**Push Provider 表**：
- `push_provider_configs` - 推送提供商配置
- `push_notification_logs` - 推送日志（已存在）

---

## 五、测试规范

### 5.1 单元测试

每个新增模块必须包含单元测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_olm_session_encryption_decryption() {
        // 测试 Olm 会话加密解密
    }

    #[test]
    fn test_worker_bus_publish_subscribe() {
        // 测试消息总线发布订阅
    }

    #[test]
    fn test_fcm_provider_send() {
        // 测试 FCM 推送发送
    }
}
```

### 5.2 集成测试

新增集成测试文件：

```
tests/integration/
├── olm_integration_tests.rs
├── worker_bus_tests.rs
└── push_provider_tests.rs
```

### 5.3 测试覆盖率要求

| 模块 | 最低覆盖率 |
|------|-----------|
| Olm 服务 | 80% |
| Worker Bus | 75% |
| Push Provider | 70% |

---

## 六、性能要求

### 6.1 E2EE 性能指标

| 指标 | 目标值 |
|------|--------|
| 密钥生成时间 | < 10ms |
| 消息加密时间 | < 5ms |
| 消息解密时间 | < 5ms |
| 会话恢复时间 | < 20ms |

### 6.2 Worker 性能指标

| 指标 | 目标值 |
|------|--------|
| 消息总线延迟 | < 10ms |
| 流位置同步延迟 | < 50ms |
| Worker 注册时间 | < 100ms |

### 6.3 Push 性能指标

| 指标 | 目标值 |
|------|--------|
| 推送发送延迟 | < 500ms |
| 批量推送吞吐量 | > 1000/s |
| 推送成功率 | > 95% |

---

## 七、安全要求

### 7.1 E2EE 安全

1. 密钥必须使用安全随机数生成器
2. 会话状态必须加密存储
3. 密钥不得记录到日志
4. 实现前向保密

### 7.2 Worker 安全

1. 复制连接必须认证
2. 消息必须签名验证
3. 敏感配置必须加密

### 7.3 Push 安全

1. 推送令牌必须安全存储
2. 推送内容不得包含敏感信息（除非端到端加密）
3. 推送失败必须记录审计日志

---

## 八、兼容性要求

### 8.1 Matrix 协议版本

- 支持 Matrix v1.11 规范
- 支持 Olm v1 协议
- 支持 Megolm v1 协议

### 8.2 客户端兼容

- Element Web/Desktop
- Element iOS
- Element Android
- 其他 Matrix 客户端

### 8.3 向后兼容

- 现有 API 端点不得破坏
- 数据库迁移必须可逆
- 配置格式向后兼容

---

## 九、文档要求

### 9.1 代码文档

所有公共 API 必须包含文档注释：

```rust
/// 创建新的 Olm 会话
///
/// # Arguments
///
/// * `their_identity_key` - 对方的身份密钥
/// * `their_one_time_key` - 对方的一次性密钥
///
/// # Returns
///
/// 返回新创建的会话和初始消息
///
/// # Errors
///
/// 如果密钥无效或会话创建失败，返回错误
///
/// # Example
///
/// ```ignore
/// let session = service.create_outbound_session(&identity_key, &one_time_key).await?;
/// ```
pub async fn create_outbound_session(...) -> Result<...>;
```

### 9.2 配置文档

更新 `config.example.yaml` 添加新配置项说明。

---

## 十、风险评估

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|----------|
| vodozemac API 变更 | 高 | 低 | 锁定版本，添加兼容性测试 |
| Redis 连接不稳定 | 中 | 中 | 实现重连机制，本地缓存降级 |
| 推送服务限流 | 中 | 高 | 实现队列缓冲，指数退避重试 |
| 数据库迁移失败 | 高 | 低 | 备份机制，回滚脚本 |

---

## 十一、验收标准

### 11.1 E2EE 验收

- [ ] Olm 会话可正常创建和恢复
- [ ] 消息加密解密正确
- [ ] 会话状态正确持久化
- [ ] 与 to_device 消息正确集成
- [ ] 单元测试覆盖率 > 80%

### 11.2 Workers 验收

- [ ] 多 Worker 实例可正常注册
- [ ] Redis Pub/Sub 消息正确传递
- [ ] 流写入器正确分配
- [ ] 负载均衡正常工作
- [ ] 集成测试通过

### 11.3 Push 验收

- [ ] FCM 推送正常发送
- [ ] APNs 推送正常发送
- [ ] Web Push 正常发送
- [ ] 推送规则正确评估
- [ ] 推送日志正确记录

---

## 十二、实施计划

### 阶段 1：E2EE 优化（第 1-2 周）

1. 实现 Olm 会话存储层
2. 扩展 OlmService 功能
3. 添加数据库迁移
4. 编写单元测试

### 阶段 2：Workers 优化（第 3-4 周）

1. 实现 Redis Pub/Sub 消息总线
2. 实现流写入器管理
3. 实现负载均衡
4. 编写集成测试

### 阶段 3：Push 优化（第 5-6 周）

1. 实现 FCM Provider
2. 实现 APNs Provider
3. 实现 Web Push Provider
4. 实现 Push Gateway 协议
5. 编写测试

---

## 附录 A：参考资料

- [Matrix 规范 - End-to-End Encryption](https://spec.matrix.org/v1.11/client-server-api/#end-to-end-encryption)
- [Matrix 规范 - Push Gateway](https://spec.matrix.org/v1.11/push-gateway-api/)
- [Synapse Workers 文档](https://element-hq.github.io/synapse/latest/workers.html)
- [vodozemac 文档](https://docs.rs/vodozemac/latest/vodozemac/)
- [Firebase Cloud Messaging 文档](https://firebase.google.com/docs/cloud-messaging)
- [Apple Push Notification Service 文档](https://developer.apple.com/documentation/usernotifications)
- [Web Push 协议](https://webpush-wg.github.io/webpush-protocol/)
