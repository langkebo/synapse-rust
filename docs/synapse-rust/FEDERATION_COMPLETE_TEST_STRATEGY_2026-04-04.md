# Federation 完整闭环测试方案

> 日期：2026-04-04  
> 文档类型：测试方案设计  
> 目标：解决 Federation "部分实现" → "已实现并验证（完整闭环）" 的关键难题

---

## 一、问题分析

### 1.1 当前状态
- ✅ 错误路径测试：已验证 auth chain 失败场景
- ✅ HTTP 端点测试：version、query、publicRooms、server keys 端点可用
- ✅ Friend federation 测试：发送/接收链路基础验证
- ❌ **跨服务器互操作闭环**：缺失真实的跨 homeserver 通信验证

### 1.2 核心难题
1. **基础设施依赖**：真实互操作需要两个独立的 homeserver 实例
2. **网络配置复杂**：需要 DNS 解析、TLS 证书、端口映射
3. **测试环境隔离**：Docker 方案失败（Homeserver1 启动失败）
4. **数据库依赖**：每个实例需要独立的 PostgreSQL + Redis

### 1.3 已尝试方案
- ❌ Docker Compose 双实例：Homeserver1 启动失败
- ❌ matrix.org 互操作：需要外部网络和数据库配置
- ✅ Friend federation 单元测试：仅验证逻辑，无真实网络通信

---

## 二、推荐方案：Mock Federation Server

### 2.1 方案概述
**核心思路**：在集成测试中创建一个轻量级的 Mock Federation Server，模拟远程 homeserver 的行为，避免真实的多实例部署。

### 2.2 方案优势
1. **无需外部依赖**：不需要 Docker、多数据库、网络配置
2. **快速执行**：测试在秒级完成，适合 CI/CD
3. **完全可控**：可以精确控制远程服务器的响应
4. **易于调试**：所有代码在同一进程中，便于断点调试
5. **覆盖核心场景**：验证协议正确性，而非基础设施配置

### 2.3 技术实现
使用 Axum 创建一个最小化的 Mock Federation Server，实现关键的 Federation API 端点：

```rust
// Mock Federation Server 实现关键端点
struct MockFederationServer {
    server_name: String,
    signing_key: SigningKey,
    router: Router,
}

impl MockFederationServer {
    fn new(server_name: &str) -> Self {
        let router = Router::new()
            .route("/_matrix/federation/v1/version", get(version))
            .route("/_matrix/key/v2/server", get(server_keys))
            .route("/_matrix/federation/v1/make_join/:room_id/:user_id", get(make_join))
            .route("/_matrix/federation/v1/send_join/:room_id/:event_id", put(send_join))
            .route("/_matrix/federation/v1/invite/:room_id/:event_id", put(invite))
            .route("/_matrix/federation/v1/send/:txn_id", put(send_transaction));
        
        Self { server_name, signing_key, router }
    }
}
```

### 2.4 测试场景设计

#### 场景 1：服务器发现与密钥交换
```rust
#[tokio::test]
async fn test_federation_server_discovery() {
    let mock_server = MockFederationServer::new("remote.test");
    let local_server = setup_test_app().await;
    
    // 1. 查询远程服务器版本
    let version = local_server.query_server_version("remote.test").await;
    assert!(version.is_ok());
    
    // 2. 获取远程服务器密钥
    let keys = local_server.query_server_keys("remote.test").await;
    assert!(keys.is_ok());
    assert!(keys.unwrap().verify_keys.len() > 0);
}
```

#### 场景 2：跨服务器房间邀请
```rust
#[tokio::test]
async fn test_federation_room_invite() {
    let mock_server = MockFederationServer::new("remote.test");
    let local_server = setup_test_app().await;
    
    // 1. 本地用户创建房间
    let (local_token, local_user_id) = register_user(&local_server, "alice").await;
    let room_id = create_room(&local_server, &local_token).await;
    
    // 2. 邀请远程用户
    let remote_user_id = "@bob:remote.test";
    let invite_result = invite_user(&local_server, &local_token, &room_id, remote_user_id).await;
    
    // 3. 验证 mock server 收到邀请请求
    assert!(mock_server.received_invite(&room_id, remote_user_id));
    
    // 4. Mock server 接受邀请
    let join_result = mock_server.accept_invite(&room_id, remote_user_id).await;
    assert!(join_result.is_ok());
}
```

#### 场景 3：跨服务器消息同步
```rust
#[tokio::test]
async fn test_federation_message_sync() {
    let mock_server = MockFederationServer::new("remote.test");
    let local_server = setup_test_app().await;
    
    // 前置：建立跨服务器房间
    let (room_id, local_user, remote_user) = setup_federated_room().await;
    
    // 1. 本地用户发送消息
    let message = "Hello from local server";
    send_message(&local_server, &room_id, &local_user, message).await;
    
    // 2. 验证 mock server 收到消息事件
    let events = mock_server.get_room_events(&room_id).await;
    assert!(events.iter().any(|e| e.content.body == message));
    
    // 3. 远程用户发送消息
    let remote_message = "Hello from remote server";
    mock_server.send_message(&room_id, &remote_user, remote_message).await;
    
    // 4. 验证本地服务器收到消息
    let local_events = get_room_messages(&local_server, &room_id).await;
    assert!(local_events.iter().any(|e| e.content.body == remote_message));
}
```

#### 场景 4：事件签名验证
```rust
#[tokio::test]
async fn test_federation_event_signature_verification() {
    let mock_server = MockFederationServer::new("remote.test");
    let local_server = setup_test_app().await;
    
    // 1. Mock server 发送正确签名的事件
    let valid_event = mock_server.create_signed_event("m.room.message", content).await;
    let result = local_server.receive_federation_event(valid_event).await;
    assert!(result.is_ok());
    
    // 2. Mock server 发送错误签名的事件
    let invalid_event = mock_server.create_unsigned_event("m.room.message", content).await;
    let result = local_server.receive_federation_event(invalid_event).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), FederationError::InvalidSignature));
}
```

#### 场景 5：状态一致性验证
```rust
#[tokio::test]
async fn test_federation_state_consistency() {
    let mock_server = MockFederationServer::new("remote.test");
    let local_server = setup_test_app().await;
    
    // 1. 建立跨服务器房间
    let (room_id, _, _) = setup_federated_room().await;
    
    // 2. 在两个服务器上执行状态变更
    change_room_name(&local_server, &room_id, "New Name").await;
    
    // 3. 查询两个服务器的房间状态
    let local_state = get_room_state(&local_server, &room_id).await;
    let remote_state = mock_server.get_room_state(&room_id).await;
    
    // 4. 验证状态一致
    assert_eq!(local_state.name, remote_state.name);
    assert_eq!(local_state.members.len(), remote_state.members.len());
}
```

---

## 三、实施计划

### 3.1 Phase 1: Mock Server 基础设施（2-3 小时）
- [ ] 创建 `MockFederationServer` 结构
- [ ] 实现核心 Federation API 端点
- [ ] 实现事件签名/验证逻辑
- [ ] 创建测试辅助函数

### 3.2 Phase 2: 核心场景测试（3-4 小时）
- [ ] 服务器发现与密钥交换
- [ ] 跨服务器房间邀请
- [ ] 跨服务器消息同步
- [ ] 事件签名验证
- [ ] 状态一致性验证

### 3.3 Phase 3: 边界与错误测试（1-2 小时）
- [ ] 网络超时处理
- [ ] 签名验证失败
- [ ] 不存在的服务器
- [ ] 格式错误的请求

### 3.4 Phase 4: 文档更新（0.5 小时）
- [ ] 更新 `FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`
- [ ] 更新 `CAPABILITY_STATUS_BASELINE_2026-04-02.md`
- [ ] 升级 Federation 状态为"已实现并验证（完整闭环）"

**总预估时间：6-10 小时**

---

## 四、备选方案对比

| 方案 | 优势 | 劣势 | 推荐度 |
|------|------|------|--------|
| Mock Federation Server | 快速、可控、无外部依赖 | 不验证真实网络通信 | ⭐⭐⭐⭐⭐ |
| Docker Compose 双实例 | 最接近真实部署 | 复杂、慢、已失败 | ⭐⭐ |
| matrix.org 互操作 | 真实外部服务器 | 需要网络、不稳定 | ⭐⭐ |
| 进程内双实例 | 较快、较真实 | 复杂度中等 | ⭐⭐⭐ |

---

## 五、验收标准

Federation 能力升级为"已实现并验证（完整闭环）"需满足：

1. ✅ 服务器发现与密钥交换测试通过
2. ✅ 跨服务器房间邀请测试通过
3. ✅ 跨服务器消息同步测试通过
4. ✅ 事件签名验证测试通过
5. ✅ 状态一致性验证测试通过
6. ✅ 至少 5 个边界/错误场景测试通过
7. ✅ 所有测试在 CI 环境中稳定通过

**最小目标**：5 个核心场景测试 + 3 个错误场景测试 = 8 个测试全部通过

---

## 六、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Mock server 与真实行为不一致 | 测试通过但生产失败 | 参考 Synapse 源码实现，确保协议正确性 |
| 签名逻辑复杂 | 实现困难 | 复用现有签名服务，只 mock 网络层 |
| 测试维护成本高 | 长期负担 | 良好的抽象和辅助函数 |

---

## 七、结论

**推荐采用 Mock Federation Server 方案**，理由：

1. **可行性高**：不依赖外部基础设施，已有失败的 Docker 方案作为对比
2. **性价比高**：6-10 小时投入即可完成完整闭环验证
3. **维护性好**：测试快速、稳定、易于调试
4. **覆盖充分**：验证协议正确性，这是 Federation 的核心

通过这个方案，可以将 Federation 从"部分实现"升级为"已实现并验证（完整闭环）"，达到与 E2EE、Admin、AppService 相同的验证水平。
