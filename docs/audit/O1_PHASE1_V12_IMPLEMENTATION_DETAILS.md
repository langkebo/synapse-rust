# O-1 Phase 1: v12 房间版本实现详细方案

**目标**: 启用 v12 房间创建，确保本地创建的事件包含完整的 PDU 字段（depth, prev_events, auth_events）

**参考**: 
- `V12_ROOM_VERSION_AND_ANIMATED_THUMBNAIL_IMPLEMENTATION_PLAN.md`
- Upstream Synapse v1.162.0rc1 (element-hq/synapse)

**当前状态**: ✅ 已完成基础配置修改（`room_versions.rs`），v12 现在是可创建的

---

## 一、问题分析

### 1.1 当前问题

根据审计文档，当前本地创建的事件存在以下问题：

```rust
// synapse-storage/src/event/create.rs:14
INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, redacts)
-- ❌ 缺少：depth, prev_events, auth_events, signatures, hashes
```

**后果**:
- 本地起源事件恒为 `MissingGraphMetadata`
- v12 对等端不接受这些事件（因为缺少必需的 PDU 字段）
- 无法与上游 Synapse v1.162+ 进行联邦

### 1.2 v12 要求

根据上游 Synapse v1.162.0rc1 和 Matrix Spec：

1. **PDU 字段完整性**:
   - `depth`: 事件图的深度（max(prev_event_depths) + 1）
   - `prev_events`: 前驱事件列表（forward extremities）
   - `auth_events`: 认证事件列表（必须包含 m.room.create, m.room.power_levels, m.room.member(creator), m.room.history_visibility）
   - `signatures`: 服务器签名
   - `hashes`: SHA256 哈希

2. **ED25519-only 验证**:
   - 拒绝非 ED25519 签名算法的事件

3. **MSC4311 合规性**:
   - `m.room.create` 必须出现在 v12 房间的 stripped invite/knock state 中

---

## 二、实施方案

### 阶段 1.1: ✅ 扩展 CreateEventParams（已完成）

**状态**: ✅ 已完成（Commit: `e55588718`）

**文件**: `synapse-storage/src/event/models.rs`

```rust
pub struct PduGraphFields {
    pub depth: Option<i64>,
    pub prev_events: Option<Vec<String>>,
    pub auth_events: Option<Vec<String>>,
}
```

**向后兼容性**: 现有调用点使用 `create_event()` 不受影响

### 阶段 1.2: ✅ 修改 create_event 支持 PDU 字段（已完成）

**状态**: ✅ 已完成（Commit: `e55588718`）

**文件**: `synapse-storage/src/event/create.rs`

```rust
pub async fn create_event_with_pdu(
    &self,
    params: CreateEventParams,
    pdu_graph: PduGraphFields,
    tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
) -> Result<RoomEvent, sqlx::Error> {
    // 写入完整 PDU 字段到数据库
}
```

### 阶段 1.3: 实现 depth 计算逻辑（TODO）

**目标**: 为新事件计算正确的 depth

**参考**: Upstream Synapse 的 `compute_depth` 逻辑

**现有资源**: 
- ✅ `get_forward_extremities_in_room()` - 获取末端事件
- ✅ `get_event_graph_fields()` - 查询事件的 PDU 字段

**实现思路**:

```rust
// synapse-storage/src/event/depth.rs (新建)
impl EventStorage {
    /// 计算事件的 depth = max(prev_events 的 depth) + 1
    /// 如果 prev_events 为空，返回 1（第一个事件）
    pub async fn calculate_event_depth(
        &self,
        room_id: &str,
        prev_events: &[String],
    ) -> Result<i64, sqlx::Error> {
        if prev_events.is_empty() {
            return Ok(1); // 第一个事件
        }
        
        // 查询 prev_events 的最大 depth
        let max_depth = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(depth), 0) FROM events WHERE room_id = $1 AND event_id = ANY($2)"
        )
        .bind(room_id)
        .bind(prev_events)
        .fetch_one(&*self.pool)
        .await?;
        
        Ok(max_depth + 1)
    }
}
```

**复杂度**: 低（单次 DB 查询）

**预估时间**: 1-2 天

### 阶段 1.4: 实现 auth_events 构造逻辑（TODO）

**目标**: 根据当前房间状态构造 auth_events

**参考**: Upstream Synapse 的 `compute_auth_events` 逻辑

**现有资源**:
- ✅ `get_state_events(room_id)` - 获取当前状态事件
- ✅ `get_state_events_by_type(room_id, event_type)` - 按类型获取状态事件

**实现思路**:

```rust
// synapse-services/src/room/auth.rs (新建或扩展现有)
pub struct AuthEventBuilder {
    room_state: HashMap<String, StateEvent>, // (event_type, state_key) -> StateEvent
}

impl AuthEventBuilder {
    /// 构造 v12 的 auth_events
    /// 必须包含：
    /// - m.room.create (必需，MSC4311)
    /// - m.room.power_levels
    /// - m.room.member (creator)
    /// - m.room.history_visibility
    pub fn build_auth_events(
        &self,
        event_type: &str,
        state_key: Option<&str>,
        creator_user_id: &str,
    ) -> Vec<String> {
        let mut auth_events = Vec::new();
        
        // 1. m.room.create (总是需要)
        if let Some(create_event) = self.room_state.get(&(
            "m.room.create".to_string(), "".to_string())) {
            auth_events.push(create_event.event_id.clone());
        }
        
        // 2. m.room.power_levels (如果存在)
        if let Some(pl_event) = self.room_state.get(&(
            "m.room.power_levels".to_string(), "".to_string())) {
            auth_events.push(pl_event.event_id.clone());
        }
        
        // 3. m.room.member (creator)
        if let Some(member_event) = self.room_state.get(&(
            "m.room.member".to_string(), creator_user_id.to_string())) {
            auth_events.push(member_event.event_id.clone());
        }
        
        // 4. m.room.history_visibility (如果存在)
        if let Some(hv_event) = self.room_state.get(&(
            "m.room.history_visibility".to_string(), "".to_string())) {
            auth_events.push(hv_event.event_id.clone());
        }
        
        auth_events
    }
}
```

**复杂度**: 中（需要理解状态事件查询）

**预估时间**: 2-3 天

### 阶段 1.5: 修改消息创建逻辑以使用 PDU 字段（TODO）

**目标**: 确保本地创建的消息事件包含完整的 PDU 字段

**文件**: `synapse-services/src/room/messaging/messages.rs`

**实现思路**:

```rust
pub async fn send_message(
    &self,
    room_id: &str,
    user_id: &str,
    content: serde_json::Value,
) -> ApiResult<SendMessageResponse> {
    // 1. 获取房间版本
    let room_version = self.get_room_version(room_id).await?;
    
    // 2. 如果是 v12+，使用完整的 PDU 字段
    if room_version >= "12" {
        // 2a. 获取当前状态（用于计算 auth_events）
        let state_events = self.event_reader.get_state_events(room_id).await?;
        let auth_builder = AuthEventBuilder::from_state(state_events);
        
        // 2b. 获取前驱事件（末端 extremities）
        let prev_events = self.event_reader
            .get_forward_extremities_in_room(room_id, 10)
            .await?;
        
        // 2c. 计算 depth
        let depth = self.event_storage
            .calculate_event_depth(room_id, &prev_events)
            .await?;
        
        // 2d. 构造 auth_events
        let auth_events = auth_builder.build_auth_events(
            "m.room.message",
            None,
            user_id,
        );
        
        // 2e. 创建事件（带完整 PDU 字段）
        let event = self.event_writer.create_event_with_pdu(
            CreateEventParams {
                event_id,
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: "m.room.message".to_string(),
                content,
                state_key: None,
                origin_server_ts: current_timestamp_millis(),
                redacts: None,
            },
            PduGraphFields {
                depth: Some(depth),
                prev_events: Some(prev_events),
                auth_events: Some(auth_events),
            },
            None,
        ).await?;
        
        // 2f. 签名并广播
        self.sign_and_broadcast_event(event).await?;
        
        return Ok(SendMessageResponse { event_id });
    }
    
    // 3. 旧版本：保持原有行为
    // ...
}
```

**复杂度**: 高（需要协调多个模块）

**预估时间**: 3-4 天

### 阶段 1.6: 添加 ED25519-only 验证（TODO）

**目标**: 对 v12 事件强制执行 ED25519 签名验证

**文件**: `synapse-services/src/auth/event_auth.rs`

**实现思路**:

```rust
pub fn validate_event_signatures(
    event: &Event,
    room_version: &str,
) -> Result<(), ApiError> {
    if room_version == "12" {
        // v12: 只允许 ED25519 签名
        for (server_name, signature) in event.signatures.iter() {
            match signature.algorithm() {
                SignatureAlgorithm::Ed25519 => continue,
                _ => return Err(ApiError::forbidden(format!(
                    "v12 rooms only support Ed25519 signatures, got {}",
                    signature.algorithm()
                ))),
            }
        }
    }
    
    // v11 及以下：允许其他签名算法（向后兼容）
    Ok(())
}
```

**复杂度**: 低（简单的模式匹配）

**预估时间**: 1-2 天

### 阶段 1.7: 测试（TODO）

**目标**: 确保 v12 房间创建和事件处理正常工作

**测试用例**:

```rust
#[tokio::test]
async fn test_v12_room_creation() {
    // 1. 创建 v12 房间
    let room_id = create_room("12").await;
    
    // 2. 发送消息
    let event_id = send_message(&room_id, "@user:test").await;
    
    // 3. 验证事件包含完整的 PDU 字段
    let event = get_event(&event_id).await;
    assert!(event.depth > 0);
    assert!(!event.prev_events.is_empty());
    assert!(!event.auth_events.is_empty());
    assert!(!event.signatures.is_empty());
    assert!(!event.hashes.is_empty());
    
    // 4. 验证上游 Synapse 接受该事件
    // （需要联邦测试环境）
}

#[tokio::test]
async fn test_v12_rejects_non_ed25519_signatures() {
    // 创建带有 RSA 签名的事件 → 应该验证失败
}
```

**复杂度**: 中（需要设置测试环境）

**预估时间**: 2-3 天

---

## 三、依赖关系

```
阶段 1.1 ✅ → 阶段 1.2 ✅ → 阶段 1.3 → 阶段 1.4 → 阶段 1.5 → 阶段 1.6 → 阶段 1.7
                                                    ↓
                                              阶段 1.4 (auth_events)
```

---

## 四、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| depth 计算不准确 | 高 | 单元测试 + 可视化验证 |
| auth_events 不完整 | 高 | 对照 upstream Synapse 逻辑 |
| 性能下降（多次 DB 查询） | 中 | 批量查询 + 缓存 |
| 与上游 Synapse 不兼容 | 高 | 联邦测试 + 对比日志 |

---

## 五、下一步行动

1. **立即开始**: 阶段 1.3（depth 计算）
2. **并行进行**: 阶段 1.4（auth_events 构造）
3. **等待依赖**: 阶段 1.5（需要 1.3 和 1.4 完成）

---

## 六、参考链接

- [Upstream Synapse v1.162.0rc1](https://github.com/element-hq/synapse/tree/v1.162.0rc1)
- [Matrix Spec - Room Versions](https://spec.matrix.org/v1.10/rooms/v12/)
- [MSC4311 - m.room.create in invite/knock state](https://github.com/matrix-org/matrix-spec-proposals/pull/4311)
**文件**: `synapse-storage/src/event/depth.rs` (新建)

```rust
impl EventStorage {
    /// 计算事件的 depth = max(prev_events 的 depth) + 1
    pub async fn calculate_event_depth(
        &self,
        room_id: &str,
        prev_events: &[String],
    ) -> Result<i64, sqlx::Error> {
        if prev_events.is_empty() {
            return Ok(1); // 第一个事件
        }
        
        // 查询 prev_events 的最大 depth
        let max_depth = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(depth), 0) FROM events WHERE room_id = $1 AND event_id = ANY($2)"
        )
        .bind(room_id)
        .bind(prev_events)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(max_depth + 1)
    }
}
```

### 阶段 1.4: 实现 auth_events 构造（3-4 天）

**目标**: 根据当前房间状态构造 auth_events

**文件**: `synapse-services/src/room/auth.rs` (新建或扩展现有)

```rust
pub struct AuthEventBuilder {
    room_state: RoomState,
}

impl AuthEventBuilder {
    /// 构造 v12 的 auth_events
    /// 必须包含：
    /// - m.room.create (必需，MSC4311)
    /// - m.room.power_levels
    /// - m.room.member (creator)
    /// - m.room.history_visibility
    pub fn build_auth_events(
        &self,
        event_type: &str,
        state_key: Option<&str>,
        creator_user_id: &str,
    ) -> Vec<String> {
        let mut auth_events = Vec::new();
        
        // 1. m.room.create (总是需要)
        if let Some(create_event) = self.room_state.get_event("m.room.create", "") {
            auth_events.push(create_event.event_id);
        }
        
        // 2. m.room.power_levels (如果存在)
        if let Some(pl_event) = self.room_state.get_event("m.room.power_levels", "") {
            auth_events.push(pl_event.event_id);
        }
        
        // 3. m.room.member (creator)
        if let Some(member_event) = self.room_state.get_event("m.room.member", creator_user_id) {
            auth_events.push(member_event.event_id);
        }
        
        // 4. m.room.history_visibility (如果存在)
        if let Some(hv_event) = self.room_state.get_event("m.room.history_visibility", "") {
            auth_events.push(hv_event.event_id);
        }
        
        auth_events
    }
}
```

### 阶段 1.5: 修改消息创建逻辑以使用 PDU 字段（5-7 天）

**目标**: 确保本地创建的消息事件包含完整的 PDU 字段

**文件**: `synapse-services/src/room/messaging/messages.rs`

```rust
pub async fn send_message(
    &self,
    room_id: &str,
    user_id: &str,
    content: serde_json::Value,
) -> ApiResult<SendMessageResponse> {
    // 1. 获取房间版本
    let room_version = self.get_room_version(room_id).await?;
    
    // 2. 获取当前状态（用于计算 auth_events）
    let state = self.get_current_state(room_id).await?;
    
    // 3. 获取前驱事件（末端 extremities）
    let prev_events = self.get_forward_extremities(room_id).await?;
    
    // 4. 计算 depth
    let depth = self.calculate_depth(room_id, &prev_events).await?;
    
    // 5. 构造 auth_events
    let auth_events = self.build_auth_events(
        "m.room.message",
        None,
        user_id,
        &state,
    ).await?;
    
    // 6. 创建事件（带完整 PDU 字段）
    let event = self.event_writer.create_event(
        CreateEventParams {
            event_id,
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            event_type: "m.room.message".to_string(),
            content,
            state_key: None,
            origin_server_ts: current_timestamp_millis(),
            redacts: None,
            depth: Some(depth),
            prev_events: Some(prev_events),
            auth_events: Some(auth_events),
        },
        None,
    ).await?;
    
    // 7. 签名并广播
    self.sign_and_broadcast_event(event).await?;
    
    Ok(SendMessageResponse { event_id })
}
```

### 阶段 1.6: 添加 ED25519-only 验证（2-3 天）

**目标**: 对 v12 事件强制执行 ED25519 签名验证

**文件**: `synapse-services/src/auth/event_auth.rs`

```rust
pub fn validate_event_signatures(
    event: &Event,
    room_version: &str,
) -> Result<(), ApiError> {
    if room_version == "12" {
        // v12: 只允许 ED25519 签名
        for (server_name, signature) in event.signatures.iter() {
            match signature.algorithm() {
                SignatureAlgorithm::Ed25519 => continue,
                _ => return Err(ApiError::forbidden(format!(
                    "v12 rooms only support Ed25519 signatures, got {}",
                    signature.algorithm()
                ))),
            }
        }
    }
    
    // v11 及以下：允许其他签名算法（向后兼容）
    Ok(())
}
```

### 阶段 1.7: 测试（3-4 天）

**目标**: 确保 v12 房间创建和事件处理正常工作

**测试用例**:

```rust
#[tokio::test]
async fn test_v12_room_creation() {
    // 1. 创建 v12 房间
    let room_id = create_room("12").await;
    
    // 2. 发送消息
    let event_id = send_message(&room_id, "@user:test").await;
    
    // 3. 验证事件包含完整的 PDU 字段
    let event = get_event(&event_id).await;
    assert!(event.depth > 0);
    assert!(!event.prev_events.is_empty());
    assert!(!event.auth_events.is_empty());
    assert!(!event.signatures.is_empty());
    assert!(!event.hashes.is_empty());
    
    // 4. 验证上游 Synapse 接受该事件
    // （需要联邦测试环境）
}

#[tokio::test]
async fn test_v12_rejects_non_ed25519_signatures() {
    // 创建带有 RSA 签名的事件 → 应该验证失败
}
```

---

## 三、依赖关系

### 内部依赖

1. **RoomState 查询**: 需要高效的当前状态查询
2. **Forward Extremities**: 需要维护 forward extremities 列表
3. **签名基础设施**: 需要现有的签名机制

### 外部依赖

1. **上游 Synapse**: 用于联邦兼容性测试
2. **PostgreSQL**: 支持 `ANY()` 数组查询

---

## 四、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| depth 计算错误 | 高 | 单元测试 + 与上游对比 |
| auth_events 不完整 | 高 | 严格遵循规范，mutation self-proof |
| 性能下降 | 中 | 缓存状态查询，批量查询 |
| 联邦不兼容 | 高 | 与上游 Synapse 测试 |

---

## 五、里程碑

### Milestone 1: 基础支持（1 周）
- [x] 修改 `room_versions.rs` 启用 v12 创建
- [ ] 扩展 `CreateEventParams`
- [ ] 修改 `create_event` 支持 PDU 字段

### Milestone 2: PDU 字段计算（2 周）
- [ ] 实现 depth 计算
- [ ] 实现 auth_events 构造
- [ ] 修改消息创建逻辑

### Milestone 3: 验证与测试（1 周）
- [ ] 添加 ED25519-only 验证
- [ ] 编写单元测试
- [ ] 联邦兼容性测试

### Milestone 4: 文档与部署（几天）
- [ ] 更新文档
- [ ] 迁移指南
- [ ] 监控指标

---

## 六、回滚计划

如果发现问题：
1. 将 `DEFAULT_ROOM_VERSION` 改回 "11"
2. 将 v12 设为 `stable_parse_only`
3. 回滚相关提交

---

**下一步行动**:
1. 开始阶段 1.1：扩展 CreateEventParams
2. 创建对应的 PR
3. 等待审查后继续下一阶段
