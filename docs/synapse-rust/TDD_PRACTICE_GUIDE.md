# synapse-rust TDD 实战指南

> 面向团队的 TDD 工作流入门与参考手册。
> 完整策略与任务分解见 [TDD落地执行清单](../../.trae/documents/TDD落地执行清单.md)。

## 快速开始

```bash
# RED: 写一个失败的测试
cargo nextest run -p synapse-storage my_new_test -P tdd

# GREEN: 最小实现让测试通过
# 然后: cargo nextest run -p synapse-storage my_new_test -P tdd

# REFACTOR: 清理代码，保持绿色
cargo clippy --all-features --locked -- -D warnings
cargo fmt --all -- --check
cargo nextest run -p synapse-storage my_new_test -P tdd
```

## 测试层次选择

| 场景 | 测试类型 | 命令 |
|------|---------|------|
| 纯函数/解析器/验证器 | `#[cfg(test)]` 内联单测 | `cargo test --lib` |
| Storage trait 新方法 | 单测 + InMemory fake | `cargo nextest run --test unit` |
| Service 业务逻辑 | 单测 + Mock deps | `cargo nextest run --test unit --features test-utils` |
| API 响应 shape | insta 快照 | `cargo nextest run --test integration --features test-utils` |
| 端到端流程 | 集成测试 + 真实 DB | `cargo nextest run --test integration --all-features` |

## Mock 适配器清单

### 存储层

| Trait | InMemory 实现 | 位置 |
|-------|--------------|------|
| `EventStoreApi` | `InMemoryEventStore` | `synapse-storage/src/test_mocks.rs` |
| `RoomStoreApi` | `InMemoryRoomStore` | `synapse-storage/src/test_mocks.rs` |
| `MemberStoreApi` | `InMemoryMemberStore` | `synapse-storage/src/test_mocks.rs` |
| `PresenceStoreApi` | `InMemoryPresenceStore` | `synapse-storage/src/test_mocks.rs` |
| `UserStore` (已有 trait) | `FakeUserStore` | `synapse-storage/src/test_mocks.rs` |

使用模式：

```rust
use synapse_storage::test_mocks::InMemoryEventStore;
use synapse_storage::event::EventStoreApi;

let store = InMemoryEventStore::new()
    .with_event(event_id, room_id, sender, /* ... */);

// 注入到 service deps
let deps = MockSyncServiceDepsBuilder::new()
    .with_event_store(Arc::new(store))
    .build();
```

### 联邦层

```rust
use synapse_federation::test_mocks::MockFederationClient;
use synapse_federation::client_api::FederationClientApi;

let client = MockFederationClient::new()
    .seed_backfill(room_id, pdus)
    .seed_make_join(room_id, response);
let client: Arc<dyn FederationClientApi> = Arc::new(client);
```

### 服务层

```rust
use synapse_services::test_mocks::{MockSyncServiceDepsBuilder, FakeAuth};

let deps = MockSyncServiceDepsBuilder::new()
    .with_fake_user_store()
    .with_in_memory_event_store()
    .build();

let auth = FakeAuth::new().with_validated_user("@alice:localhost");
```

## insta 快照测试

### 添加新快照

```rust
#[tokio::test]
async fn my_endpoint_response_shape_is_stable() {
    let response = call_handler(/* ... */).await;
    tests::common::snapshots::assert_json_snapshot("my_endpoint_success", &response);
}
```

### 工作流

```bash
# 首次生成 / 更新快照
cargo test --test integration my_endpoint -- --exact --nocapture
cargo insta review   # 人工确认差异

# CI 验证快照无漂移
cargo insta test --no-review --test-runner nextest -- --all-features
```

### Redaction 规则

`tests/common/snapshots.rs` 自动遮蔽以下动态字段：

- `access_token`, `refresh_token`, `device_id` → `[REDACTED]`
- `origin_server_ts`, `age`, `valid_until_ts` → `[TIMESTAMP]`
- RFC 4122 UUID → `[UUID]`
- `event_id` 格式 → `[EVENT_ID]`
- `room_id` 格式 → `[ROOM_ID]`

## Trait Seam 模式

当需要为已有 struct mock 时，按以下顺序：

1. **定义 trait**（与 impl 同文件）
2. **委托 impl**（ConcreteType → trait）
3. **添加 InMemory/Mock impl**（test_mocks.rs）
4. **更新调用方**（`ConcreteType` → `Arc<dyn Trait>`）

```rust
// Step 1: 定义 trait
#[async_trait]
pub trait MyStoreApi: Send + Sync {
    async fn get_thing(&self, id: &str) -> Result<Option<Thing>, Error>;
}

// Step 2: 委托
#[async_trait]
impl MyStoreApi for MyStore {
    async fn get_thing(&self, id: &str) -> Result<Option<Thing>, Error> {
        self.get_thing(id).await  // 现有方法
    }
}

// Step 3: InMemory fake
pub struct InMemoryMyStore { things: RwLock<HashMap<String, Thing>> }

#[async_trait]
impl MyStoreApi for InMemoryMyStore {
    async fn get_thing(&self, id: &str) -> Result<Option<Thing>, Error> {
        Ok(self.things.read().await.get(id).cloned())
    }
}

// Step 4: 调用方接受 Arc<dyn MyStoreApi>
```

## CI 守门规则

| 守门 | 触发条件 | 阻断级别 |
|------|---------|---------|
| `cargo fmt --check` | pre-commit + CI | 阻断 |
| `cargo clippy -D warnings` | pre-commit + CI | 阻断 |
| `cargo insta test --no-review` | 集成测试阶段 | 阻断 |
| 文件覆盖率阈值 | `coverage` job | 阻断 |
| `cargo deny check` | pre-push + CI | 阻断 |
| `cargo audit` | pre-commit | 阻断 |
