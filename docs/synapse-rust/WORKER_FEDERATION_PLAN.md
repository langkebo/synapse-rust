# synapse-rust Worker & Federation 优化方案

> **制定日期**: 2026-03-15

---

## 一、现状分析

### 1.1 Worker 模块

**代码存在**: ✅ 完整模块 `src/worker/`

| 组件 | 状态 | 说明 |
|------|------|------|
| WorkerManager | ✅ | 完整的生命周期管理 |
| WorkerBus | ✅ | Redis 总线通信 |
| LoadBalancer | ✅ | 负载均衡 |
| Protocol | ✅ | 复制协议 |
| Storage | ✅ | Worker 存储 |

**使用状态**: ❌ 未在主程序中启用

### 1.2 Federation 模块

**路由存在**: ✅ 完整实现 `src/web/routes/federation.rs`

已实现的 Federation 端点:

| 端点 | 状态 |
|------|------|
| `/_matrix/federation/v1/version` | ✅ |
| `/_matrix/federation/v1/publicRooms` | ✅ |
| `/_matrix/federation/v2/server` | ✅ |
| `/_matrix/key/v2/server` | ✅ |
| `/_matrix/federation/v1/keys/claim` | ✅ |
| `/_matrix/federation/v1/keys/upload` | ✅ |
| `/_matrix/federation/v1/send/{txn_id}` | ✅ |
| `/_matrix/federation/v1/make_join` | ✅ |
| `/_matrix/federation/v1/make_leave` | ✅ |
| `/_matrix/federation/v1/invite` | ✅ |
| `/_matrix/federation/v1/backfill` | ✅ |
| `/_matrix/federation/v1/state` | ✅ |
| `/_matrix/federation/v1/event_auth` | ✅ |
| `/_matrix/federation/v1/query/auth` | ✅ |

---

## 二、官方 Synapse Worker 架构

### 2.1 Worker 类型

根据 Synapse 官方文档:

1. **Main Process** - 主进程处理:
   - 注册/登录
   - 用户目录
   - Account Data

2. **Reader Workers** - 读取:
   - `/sync` - 同步
   - `/events` - 事件
   - `/rooms/{id}/messages` - 消息历史

3. **Writer Workers** - 写入:
   - `/send_transaction` - 发送事务
   - Typing/Presence 指示器

4. **Special Workers**:
   - Media Repository - 媒体服务
   - Federation Sender - 联邦发送
   - Background Tasks - 后台任务

### 2.2 通信机制

- **Redis Pub/Sub**: Worker 间通信
- **HTTP Replication**: 事件复制
- **Stream Writers**: 事件持久化

---

## 三、优化计划

### 3.1 短期 (1-2周): 完善 Federation

#### 补充缺失的 Federation 端点

| 端点 | 优先级 | 说明 |
|------|--------|------|
| `/get_missing_events` | 🔴 高 | 获取缺失事件 |
| `/send_join` | 🔴 高 | 发送加入 |
| `/send_leave` | 🔴 高 | 发送离开 |
| `/exchange_third_party_invite` | 🟡 中 | 第三方邀请 |
| `/hierarchy` | 🟡 中 | 房间层级 |
| `/timestamp_to_event` | 🟢 低 | 时间戳转换 |

### 3.2 中期 (2-4周): 启用 Worker 模块

#### Worker 启用步骤

1. **配置 Redis 连接**
   ```rust
   // 在 server.rs 中初始化
   let redis_config = RedisConfig {
       host: "localhost".to_string(),
       port: 6379,
       ..default_config()
   };
   ```

2. **初始化 Worker Manager**
   ```rust
   let worker_manager = WorkerManager::new(
       worker_storage,
       server_name.clone()
   ).with_bus(redis_config, instance_name);
   ```

3. **注册 Worker 端点**
   - Sync Reader
   - Event Writer
   - Typing Handler
   - Presence Handler

### 3.3 长期 (4-8周): 完整 Worker 架构

根据官方文档实现的 Worker 架构:

1. **Stream Writers**:
   - Events Stream
   - Typing Stream
   - Presence Stream
   - Receipts Stream
   - Account Data Stream

2. **水平扩展**:
   - 多个 Sync Workers (负载均衡)
   - 多个 Event Persisters (房间分片)
   - Federation Sender (出站联邦)

---

## 四、当前配置建议

### 4.1 推荐的 Federation 配置

```yaml
# homeserver.yaml
federation:
  enabled: true
  port: 8448
  server_name: cjystx.top
  
# 签名密钥
signing_key_path: signing.key

# 信任的密钥服务器
trusted_key_servers:
  - matrix.org
```

### 4.2 Nginx 反向代理配置

```nginx
# Federation 端口
server {
    listen 8448 ssl;
    server_name cjystx.top;
    
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    location /_matrix/federation {
        proxy_pass http://localhost:8008;
    }
}
```

---

## 五、任务清单

### Federation 完善

- [x] `/get_missing_events` ✅ 已实现
- [x] `/send_join` ✅ 已实现
- [x] `/send_leave` ✅ 已实现
- [x] `/hierarchy` ✅ 已添加
- [x] `/timestamp_to_event` ✅ 已添加

### E2EE 完善

- [x] `/keys/upload` ✅ 已实现
- [x] `/keys/query` ✅ 已实现
- [x] `/keys/claim` ✅ 已实现
- [x] `/keys/changes` ✅ 已实现
- [x] `/keys/signatures/upload` ✅ 已实现
- [x] `/keys/device_signing/upload` ✅ 已实现
- [x] `/sendToDevice` ✅ 已实现
- [x] `/room_keys/distribution` ✅ 已实现

### Worker 启用

- [ ] Redis 配置集成
- [ ] Worker Manager 初始化
- [ ] 健康检查端点
- [ ] 基本的 Reader Worker

---

## 六、总结

| 模块 | 当前状态 | 完善计划 |
|------|----------|----------|
| Federation | 80% 完整 | 补充关键端点 |
| Worker | 代码存在 | 启用并配置 |
| 整体 | 可用 | 逐步完善 |

**建议**: 
1. 先完善 Federation 端点 (短期)
2. 再启用 Worker 基础功能 (中期)
3. 最后实现完整水平扩展 (长期)
