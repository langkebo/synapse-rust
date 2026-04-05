# Task 13 - 服务聚合演进方案

## 1. 背景

当前 `ServiceContainer` 同时暴露大量 storage/service，调用方容易在 handler 中直接拼装跨域依赖，增加维护成本与测试注入复杂度。

## 2. 目标聚合

| 聚合 | 主要成员 | 面向调用方暴露的职责 |
| --- | --- | --- |
| `RoomServices` | `room_service`, `room_storage`, `member_storage`, `event_storage` | 房间生命周期、成员关系、房间只读查询、state 访问 |
| `RoomAccessServices` | `room_storage`, `member_storage`, `auth_service` | `RoomContext` 构建与 guard 查询 |
| `E2eeServices` | `megolm_service`, `olm_service`, `device_storage`, `device_trust_service`, backup services | 设备 key、room key、trust、backup |
| `SearchServices` | `search_service`, `search_index_storage`, query adapters | 搜索 DSL 与 provider 执行 |
| `ModerationServices` | moderation storage/service, reporting paths | 举报、封禁、审计 |
| `SchemaGateServices` | schema validator, checksum, migration audit helpers | schema contract 与 migration gate |

## 3. 演进阶段

### 阶段 1：Facade 聚合
- 不改底层服务构造方式。
- 在 `ServiceContainer` 上新增只读聚合访问器，例如 `services.room()`、`services.e2ee()`。

最小落地草图（只读 facade）：

```rust
pub struct RoomServices<'a> {
    pub room_service: &'a RoomService,
    pub room_storage: &'a RoomStorage,
    pub member_storage: &'a MemberStorage,
    pub event_storage: &'a EventStorage,
}

impl ServiceContainer {
    pub fn room(&self) -> RoomServices<'_> {
        RoomServices {
            room_service: &self.room_service,
            room_storage: &self.room_storage,
            member_storage: &self.member_storage,
            event_storage: &self.event_storage,
        }
    }
}
```

### 阶段 2：调用方收敛
- 新代码优先依赖聚合而非零散字段。
- 首批试点放在 `room` 和 `e2ee` 两个高耦合域。

### 阶段 3：测试替身收敛
- 为聚合提供 fake/mock 入口，简化集成测试与单元测试初始化。

测试注入口径（最小要求）：
- 单元测试：允许以 `RoomServices<'_>` 为边界注入 fake（例如 `FakeRoomStorage`、`FakeMemberStorage`），避免必须构造完整 `ServiceContainer`。
- 集成测试：仍使用真实容器，但 handler 层依赖应聚合化，降低初始化噪音。

### 阶段 4：容器字段降噪
- 当 80% 以上调用方完成迁移后，再评估隐藏部分底层字段或标记为内部实现。

## 4. 试点范围

- `Task 12` 拆分后的 `room/access.rs`, `room/membership.rs`, `room/state.rs`
- `e2ee/trust.rs`, `e2ee/keys.rs`
- 后续 `search` 重构的统一入口

## 5. 验收标准

- 新增聚合后，handler 依赖列表明显缩短。
- 首批试点模块不再直接引用 5 个以上零散服务字段。
- 测试初始化可按聚合注入，而不是全量容器耦合。
