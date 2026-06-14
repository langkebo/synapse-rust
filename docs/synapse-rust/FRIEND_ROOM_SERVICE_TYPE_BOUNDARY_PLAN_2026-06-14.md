# friend_room_service Type Boundary Plan

> 版本: v0.3
> 日期: 2026-06-14
> 对应模块:
> - `src/services/friend_room_service/*`
> - `synapse-services/src/friend_room_service/*`
> - `src/services/container.rs`
> - `src/services/room/service.rs`
> - `synapse-services/src/room/service.rs`

---

## 一、目标

本文档用于收敛 `P1-03` 当前最关键的剩余阻断：为什么 `src/services/friend_room_service` 还不能直接收口为 canonical facade，以及下一步应该先改什么、后改什么。

当前已完成第一阶段代码落地：

- canonical `friend_room_service` 已引入窄房间接口 `FriendRoomRoomOps`
- canonical `friend_room_service` 已引入联邦接口 `FriendFederationSender`
- canonical `friend_room_service` 已引入中性 DTO `FriendRoomCreateRoomConfig`
- root `src/services/friend_room_service/mod.rs` 已收口为薄包装，转由 canonical 实现承载主逻辑
- root `src/web/routes/dm.rs` 与 `src/web/routes/friend_room.rs` 已切换到 `FriendRoomCreateRoomConfig`

因此本文档不再只是“方案草案”，而是“已落地第一阶段后的后续治理说明”。

---

## 二、当前结论

`friend_room_service` 已不再是单纯的 DTO 漂移问题，真正阻断 root 侧直接 facade 化的是 **构造参数和公开方法签名仍绑定在 root/canonical 各自独立的具体类型上**。

其中最核心的发现有两个：

- canonical `FriendRoomService` 对 `RoomService` 的实际耦合面很窄，当前只依赖 `create_room(...)` 与 `create_event(...)` 两个能力。
- 当前阻断并不在 `FriendRoomStorage` / `UserStorage` / `PresenceStorage` 这类已基本对齐的依赖，而在 `RoomService`、`CreateRoomConfig`、`KeyRotationManager` 这些仍暴露为具体类型的边界。

---

## 三、类型边界矩阵

| 依赖/类型 | 当前状态 | 是否阻断直接 facade | 说明 |
|---|---|---|---|
| `FriendRoomStorage` | 已基本对齐 | 否 | root 侧已能稳定传入，对当前 facade 尝试不是阻断点。 |
| `UserStorage` | 已基本对齐 | 否 | root/canonical 在装配面已可兼容使用。 |
| `PresenceStorage` | 已基本对齐 | 否 | 当前不是关键阻断。 |
| `CacheManager` | 已有适配器 | 否 | root `CacheManager` 已提供 `to_synapse_cache_manager()`，说明该边界可转换。 |
| `RoomService` | 具体类型硬绑定 | 是 | canonical 构造函数要求 `Arc<synapse_services::RoomService>`，root 当前只能提供 `Arc<crate::services::room::service::RoomService>`。 |
| `CreateRoomConfig` | 公开方法签名硬绑定 | 是 | `ensure_direct_room(...)` / `create_or_reuse_direct_message_room(...)` 直接暴露 root/canonical 各自的 `CreateRoomConfig`。 |
| `KeyRotationManager` | 构造期硬绑定 | 是 | 当前 `new(...)` 直接要求 canonical `Arc<KeyRotationManager>`，root 只有自己的同名类型。 |
| `FriendFederationClient` | 可下沉为注入依赖 | 间接 | 现在 root/canonical 都是在 `new(...)` 内用 `KeyRotationManager` 构造 client；若改为直接注入 client，可去掉一个类型边界。 |

---

## 四、真实阻断点

### 4.1 `RoomService` 是当前第一阻断

虽然 root/canonical 两侧 `RoomService` 结构很接近，但当前 `FriendRoomService` 并不只是“持有某个能建房和发 state event 的对象”，而是明确持有具体 `RoomService` 类型。

这意味着：

- 只要 constructor 还是 `Arc<RoomService>` 具体类型，就无法直接拿 root `RoomService` 去构造 canonical `FriendRoomService`。
- 即使 `CacheManager` 和其他 storage 已经可转换，也仍会被这个边界卡住。

但本轮进一步确认，`friend_room_service` 对房间服务的依赖面其实很窄：

- `create_room(...)`
- `create_event(...)`

因此下一步不应继续追求“先统一整个 `RoomService` 类型”，而应该优先把这里收敛成窄接口。

### 4.2 `CreateRoomConfig` 是公开 API 阻断

当前 root 路由会直接调用：

- `ensure_direct_room(...)`
- `create_or_reuse_direct_message_room(...)`

并传入 root `CreateRoomConfig`。

如果直接 re-export canonical `FriendRoomService`：

- 这些调用点会立刻要求 canonical `CreateRoomConfig`
- 现有 root 路由全部会产生类型不匹配

所以这里不能只改 constructor，还必须处理公开方法上的配置 DTO。

### 4.3 `KeyRotationManager` 其实是“构造链路阻断”，不是业务主阻断

`KeyRotationManager` 当前只用于 `FriendRoomService::new(...)` 内部构造 `FriendFederationClient`。

这意味着它和 `RoomService` 不同：

- 它不是业务主流程里的高频调用边界
- 更像是 constructor 设计把内部依赖泄漏成了外部强类型耦合

因此这里最合适的改法不是先做 root/canonical `KeyRotationManager` 完全统一，而是把 constructor 改为允许直接注入 `FriendFederationClient`。

---

## 五、当前已完成阶段

当前已落地：

1. 在 canonical `friend_room_service` 中定义了 `FriendRoomRoomOps`
2. 在 canonical `friend_room_service` 中定义了 `FriendFederationSender`
3. 在 canonical `friend_room_service` 中定义了 `FriendRoomCreateRoomConfig`
4. root `RoomService` / root `FriendFederationClient` 已实现上述 trait
5. root `friend_room_service` 已改为薄包装，当前仅负责：
   - 注入 root 依赖
   - root `RoomService` 对中性 DTO 的 `Into<CreateRoomConfig>` 适配
   - 保留 root 侧服务类型名与构造入口
6. root `src/web/routes/dm.rs` 与 `src/web/routes/friend_room.rs` 也已直接切换到 `FriendRoomCreateRoomConfig`

这一步的直接效果是：

- `friend_room_service` 主实现回到 canonical 单一事实来源
- root 侧整份复制实现已被删除
- root 包装层已不再保留 `ensure_direct_room(...)` / `create_or_reuse_direct_message_room(...)` 的兼容转发
- `cargo check --workspace --all-features --locked` 已恢复通过

---

## 六、建议改造顺序

### 6.1 第一步：抽出 `FriendRoomRoomOps`

在 canonical `friend_room_service` 内引入窄接口，例如：

- `create_room(owner_user_id, config)`
- `create_event(params, txn_id)`

然后：

- canonical `RoomService` 实现该接口
- root `RoomService` 也实现同名接口

这样 `FriendRoomService` 对房间能力的依赖就从“具体类型”收敛为“窄能力接口”。

### 6.2 第二步：引入中性 `FriendRoomCreateRoomConfig`

不要继续在 `friend_room_service` 的公开方法里直接暴露 `room::service::CreateRoomConfig`。

建议做法：

- 在 `friend_room_service` 自己的模块下定义中性 DTO，例如 `FriendRoomCreateRoomConfig`
- 为 root / canonical `CreateRoomConfig` 分别提供 `From` / `Into` 转换

这样可以把 DM/friend 领域所需的最小建房配置固定下来，避免继续被房间服务的整套配置类型牵着走。

### 6.3 第三步：给 constructor 增加 `new_with_federation_client(...)`

把当前：

- `server_name`
- `Arc<KeyRotationManager>`

这组构造参数降级为内部默认构造路径。

同时新增：

- `new_with_federation_client(...)`

这样 root/canonical 都能先各自构造好 `FriendFederationClient`，再注入 `FriendRoomService`，从而移除 `KeyRotationManager` 的跨 crate 类型阻断。

### 6.4 第四步：最后再评估 root 文件是否还能保留薄包装

做完前三步后，再决定：

- 是否可以把 root `friend_room_service` 完全收口为 re-export
- 还是保留极薄的 root wrapper，仅承担 root 构造/依赖适配

这一步应以编译门禁和路由调用面是否真正简化为判断标准，而不是以“必须 raw re-export”作为唯一目标。

---

## 六、不建议的路径

当前不建议继续做以下尝试：

1. 再次直接把 root `friend_room_service/mod.rs` 替换成整文件 `pub use synapse_services::friend_room_service::*;`
2. 先强行统一整个 root/canonical `RoomService`
3. 为了让 facade 勉强成立，在 root 路由层零散手写大量 config 转换

原因：

- 第 1 种已经被本轮实验明确证明会导致系统性编译失败。
- 第 2 种范围过大，不符合 `P1-03` 先收窄边界的节奏。
- 第 3 种会把问题从服务边界治理变成调用点污染，反而放大技术债。

---

## 七、下一步可执行项

按优先级建议继续：

1. 继续压薄 root `friend_room_service/mod.rs`，让其更接近纯 facade，而不是长期停留在 adapter wrapper
2. 评估是否把 root `FriendRoomService::new(...)` 的适配责任继续下沉到更合适的装配点，减少重叠文件中的非导出代码
3. 评估是否把 `FriendFederationSender` 命名和方法面再收窄，避免抽象泄漏到非必要能力
4. 视编译与调用面收益，决定是否把 root 包装进一步收口为更纯的 re-export + helper
5. 将本文件与综合审计报告持续同步，避免 `friend_room_service` 再次出现“代码已变、文档未变”的漂移
