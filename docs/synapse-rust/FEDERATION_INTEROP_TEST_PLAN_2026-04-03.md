# Federation 跨服务器互操作测试方案

> 日期：2026-04-03  
> 文档类型：测试实施方案  
> 说明：本文档定义如何实现 Federation 跨 homeserver 互操作测试

## 一、当前状态

根据 `FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`，当前 Federation 验证证据包括：

- ✅ 错误路径：auth chain 验证失败场景
- ✅ HTTP 端点：version、query、publicRooms、server keys
- ✅ 发送/接收链路：friend federation 发送与接收
- ❌ **跨服务器互操作闭环**：缺少真实或准真实的跨 homeserver 互操作验证

当前 `tests/e2e/e2e_scenarios.rs` 中的 federation 测试只是占位测试（`assert!(true)`）。

## 二、测试目标

验证两个独立的 synapse-rust 实例能够：

1. 完成服务器发现（.well-known 或 SRV 记录）
2. 建立 TLS 连接并验证服务器证书
3. 交换服务器密钥（`/_matrix/key/v2/server`）
4. 发送和接收联邦事件
5. 处理房间邀请和加入
6. 同步房间状态

## 三、实施方案

### 方案 A：Docker Compose 双实例测试（推荐）

使用 docker-compose 启动两个独立的 synapse-rust 实例，模拟真实的跨服务器场景。

#### 优点
- 最接近真实部署场景
- 可以测试完整的网络栈（DNS、TLS、HTTP）
- 可以验证服务器发现机制
- 可以测试证书验证

#### 缺点
- 需要额外的 Docker 配置
- 测试运行时间较长
- 需要管理两个独立的数据库

#### 实施步骤

1. 创建 `docker-compose.federation-test.yml`：

```yaml
version: '3.8'

services:
  homeserver1:
    build: .
    environment:
      - SERVER_NAME=server1.test
      - DATABASE_URL=postgresql://postgres:password@db1:5432/synapse1
      - FEDERATION_PORT=8448
    ports:
      - "8008:8008"
      - "8448:8448"
    depends_on:
      - db1
    networks:
      federation_test:
        aliases:
          - server1.test

  homeserver2:
    build: .
    environment:
      - SERVER_NAME=server2.test
      - DATABASE_URL=postgresql://postgres:password@db2:5432/synapse2
      - FEDERATION_PORT=8448
    ports:
      - "8009:8008"
      - "8449:8448"
    depends_on:
      - db2
    networks:
      federation_test:
        aliases:
          - server2.test

  db1:
    image: postgres:15
    environment:
      - POSTGRES_PASSWORD=password
      - POSTGRES_DB=synapse1
    networks:
      - federation_test

  db2:
    image: postgres:15
    environment:
      - POSTGRES_PASSWORD=password
      - POSTGRES_DB=synapse2
    networks:
      - federation_test

networks:
  federation_test:
    driver: bridge
```

2. 创建测试脚本 `tests/federation_interop_test.sh`：

```bash
#!/bin/bash
set -e

echo "Starting federation interop test..."

# 1. Start both homeservers
docker-compose -f docker-compose.federation-test.yml up -d
sleep 10

# 2. Register users on both servers
USER1_TOKEN=$(curl -X POST http://localhost:8008/_matrix/client/v3/register \
  -H "Content-Type: application/json" \
  -d '{"username":"user1","password":"test123"}' | jq -r .access_token)

USER2_TOKEN=$(curl -X POST http://localhost:8009/_matrix/client/v3/register \
  -H "Content-Type: application/json" \
  -d '{"username":"user2","password":"test123"}' | jq -r .access_token)

# 3. User1 creates a room
ROOM_ID=$(curl -X POST http://localhost:8008/_matrix/client/v3/createRoom \
  -H "Authorization: Bearer $USER1_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Federation Test Room"}' | jq -r .room_id)

echo "Created room: $ROOM_ID"

# 4. User1 invites User2 (cross-server invite)
curl -X POST "http://localhost:8008/_matrix/client/v3/rooms/$ROOM_ID/invite" \
  -H "Authorization: Bearer $USER1_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"@user2:server2.test"}'

echo "Sent cross-server invite"

# 5. User2 accepts invite
curl -X POST "http://localhost:8009/_matrix/client/v3/rooms/$ROOM_ID/join" \
  -H "Authorization: Bearer $USER2_TOKEN" \
  -H "Content-Type: application/json"

echo "User2 joined room"

# 6. User1 sends a message
curl -X PUT "http://localhost:8008/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/txn1" \
  -H "Authorization: Bearer $USER1_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"msgtype":"m.text","body":"Hello from server1"}'

echo "Sent message from server1"

# 7. User2 syncs and verifies message received
sleep 2
SYNC_RESPONSE=$(curl -X GET "http://localhost:8009/_matrix/client/v3/sync" \
  -H "Authorization: Bearer $USER2_TOKEN")

if echo "$SYNC_RESPONSE" | jq -e ".rooms.join[\"$ROOM_ID\"]" > /dev/null; then
  echo "✓ PASS: User2 received room state from server1"
else
  echo "✗ FAIL: User2 did not receive room state"
  exit 1
fi

# 8. Cleanup
docker-compose -f docker-compose.federation-test.yml down -v

echo "Federation interop test completed successfully"
```

### 方案 B：进程内双实例测试

在同一个测试进程中启动两个 Axum 应用实例，使用不同的端口和数据库。

#### 优点
- 更快的测试执行
- 更容易调试
- 不需要 Docker

#### 缺点
- 不能测试真实的网络栈
- 不能测试 DNS 解析和证书验证
- 需要模拟部分网络行为

#### 实施步骤

创建 `tests/integration/federation_interop_tests.rs`：

```rust
use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn test_cross_server_room_invite() {
    // Setup server1
    let pool1 = setup_test_pool("server1.test").await;
    let app1 = setup_test_app_with_pool(pool1, "server1.test").await;
    
    // Setup server2
    let pool2 = setup_test_pool("server2.test").await;
    let app2 = setup_test_app_with_pool(pool2, "server2.test").await;
    
    // Register user1 on server1
    let user1_token = register_user(&app1, "user1", "test123").await;
    
    // Register user2 on server2
    let user2_token = register_user(&app2, "user2", "test123").await;
    
    // User1 creates a room
    let room_id = create_room(&app1, &user1_token, "Test Room").await;
    
    // User1 invites user2@server2.test (cross-server)
    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", user1_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "user_id": "@user2:server2.test"
            })
            .to_string(),
        ))
        .unwrap();
    
    let response = app1.clone().oneshot(invite_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Verify server2 received the invite via federation
    // This would require mocking the federation sender/receiver
    
    // User2 accepts the invite
    let join_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", user2_token))
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap();
    
    let response = app2.clone().oneshot(join_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Verify both servers have consistent room state
}
```

### 方案 C：混合方案（推荐用于 CI）

在 CI 环境中使用 Docker Compose，在本地开发中使用进程内测试。

## 四、验收标准

无论使用哪种方案，测试必须验证：

1. **服务器发现**：server1 能够发现 server2 的联邦端点
2. **密钥交换**：两个服务器能够交换和验证服务器密钥
3. **跨服务器邀请**：server1 的用户能够邀请 server2 的用户
4. **跨服务器加入**：server2 的用户能够加入 server1 的房间
5. **事件同步**：两个服务器能够同步房间事件
6. **状态一致性**：两个服务器对房间状态达成一致

## 五、实施优先级

1. **P0**：方案 A（Docker Compose）- 创建基础配置和脚本
2. **P1**：实现跨服务器房间邀请和加入测试
3. **P2**：实现事件同步和状态一致性验证
4. **P3**：方案 B（进程内）- 用于快速本地测试

## 六、后续步骤

1. 创建 `docker-compose.federation-test.yml`
2. 创建 `tests/federation_interop_test.sh`
3. 在 CI 中添加 federation interop 测试步骤
4. 更新 `FEDERATION_VERIFICATION_MAPPING_2026-04-03.md` 标记互操作测试已完成
5. 更新 `CAPABILITY_STATUS_BASELINE_2026-04-02.md` 将 Federation 从"部分实现"升级为"已实现并验证"

## 七、参考资料

- Matrix Federation API 规范：https://spec.matrix.org/v1.11/server-server-api/
- 现有 federation 测试：`tests/integration/federation_error_tests.rs`
- Friend federation 测试：`tests/friend_federation_test.rs`
