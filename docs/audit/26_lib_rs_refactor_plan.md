# Deep lib.rs 聚合器重构技术方案

> 版本: v1.0
> 日期: 2026-07-23
> 基于 codebase-design 深度模块设计原则

---

## 一、问题分析

### 1.1 现状概览

| 文件 | `pub mod` | `pub use` | 总行数 | 问题描述 |
|------|-----------|-----------|--------|----------|
| `synapse-services/src/lib.rs` | ~76 | ~30 | ~283 | 模块声明 + re-export 混杂，新增模块需编辑此处 |
| `synapse-storage/src/lib.rs` | ~75 | ~61 | ~562 | re-export 比模块声明还多，lib.rs 成为"god module" |

### 1.2 用 codebase-design 术语分析

当前 lib.rs 是一个 **浅模块（shallow module）**：

```
┌──────────────────────────────────────────────┐
│  巨大的 Interface（130+ 个 re-export）       │  ← 消费者需要面对的 API 面
├──────────────────────────────────────────────┤
│  薄 Implementation（纯 re-export，无逻辑）    │  ← lib.rs 本身不包含业务逻辑
└──────────────────────────────────────────────┘
```

**删除测试（Deletion test）**：如果删除 lib.rs 的 re-export：
- 复杂度会重新出现在每个消费者处（需写 `synapse_storage::user::UserStorage` 而非 `synapse_storage::UserStorage`）
- 说明 re-export **有价值**（提供 leverage），但当前**过度扁平化**

### 1.3 核心痛点

1. **维护负担**：每次新增/修改模块，需同时编辑模块文件和 lib.rs
2. **认知负担**：lib.rs 280+ 行全是声明和 re-export，难以快速定位
3. **命名冲突风险**：130+ 个类型在同一个命名空间，容易出现 `ambiguous_glob_reexports`
4. **域边界模糊**：admin、e2ee、sync、media 等域的类型混在一起，无法从 import 路径看出域归属

### 1.4 Rust 2024 Edition 模块系统评估

**结论：Rust 2024 edition 未引入模块系统新特性。** 模块系统自 Rust 2018 edition 起稳定。

但以下最佳实践在 2024 仍然适用且值得应用：

| 实践 | 说明 | 当前项目应用情况 |
|------|------|-----------------|
| 域分组模块 | 将相关模块组织到子模块 | ❌ 全部扁平 |
| Prelude 模式 | `pub mod prelude { pub use ... }` 供消费者 glob | ❌ 未使用 |
| 显式 re-export | `pub use module::Type;` 而非 `pub use module::*;` | ✅ storage 已用显式 |
| `#[doc(inline)]` | 控制文档展示方式 | ❌ 未使用 |
| 避免跨 crate glob | `pub use crate::*` 难以追踪 | ⚠️ services 有 `pub(crate) use common::*` |

---

## 二、方案设计

### 2.1 设计目标

用 codebase-design 原则指导：

1. **深度优先**：每个域分组模块应有小 interface + 深层 implementation
2. **缝合位置（seam）**：域分组模块是自然的 seam，新增模块只需编辑域文件
3. **局部性（locality）**：相关模块的变更集中在域文件内，不扩散到 lib.rs
4. **向后兼容**：现有 `use synapse_storage::UserStorage` 必须继续工作

### 2.2 重构策略：域分组 + Prelude 模式 + 向后兼容

```
重构前（扁平结构）                          重构后（域分组结构）
┌─────────────────────────────┐           ┌─────────────────────────────────┐
│ lib.rs (280+ 行)             │           │ lib.rs (~80 行，仅声明域模块)    │
│  pub mod user;               │           │  pub mod admin;                 │
│  pub mod device;             │           │  pub mod e2ee;                  │
│  pub mod room;               │    ──►    │  pub mod sync;                  │
│  pub mod media;              │           │  pub mod media;                 │
│  pub mod admin_audit;        │           │  pub mod federation;            │
│  pub mod admin_user;         │           │  pub mod room;                  │
│  ... (76 个 pub mod)         │           │  ... (8 个 pub mod)              │
│  pub use user::UserStorage;  │           │  pub use prelude::*; // 兼容    │
│  pub use device::Device;     │           └─────────────────────────────────┘
│  ... (61 个 pub use)         │
└─────────────────────────────┘           ┌─────────────────────────────────┐
                                          │ admin/mod.rs                    │
                                          │  pub use crate::admin_audit::*; │
                                          │  pub use crate::admin_user::*;  │
                                          │  ... (8 个 admin_* 模块)         │
                                          └─────────────────────────────────┘
                                          
                                          ┌─────────────────────────────────┐
                                          │ prelude.rs                      │
                                          │  pub use crate::admin::*;       │
                                          │  pub use crate::e2ee::*;        │
                                          │  ... (所有域的 glob)             │
                                          └─────────────────────────────────┘
```

### 2.3 域分组方案

基于 ServiceContainer 已有的 4 层架构和模块命名模式，识别出以下域：

#### synapse-storage 域分组（8 组）

| 域 | 包含模块 | 域文件 | 类型数 |
|----|----------|--------|--------|
| **admin** | admin_federation, admin_media, audit | `admin/mod.rs` | ~15 |
| **auth** | user, device, token, threepid, captcha, refresh_token, openid_token | `auth/mod.rs` | ~25 |
| **room** | room, room_account_data, room_summary, room_tag, membership, state_groups, thread, relations, sticky_event | `room/mod.rs` | ~30 |
| **media** | media, media_quota | `media/mod.rs` | ~15 |
| **federation** | federation_blacklist, federation_queue | `federation/mod.rs` | ~10 |
| **sync** | filter, sliding_sync, presence | `sync/mod.rs` | ~15 |
| **e2ee** | dehydrated_device, e2ee_audit | `e2ee/mod.rs` | ~5 |
| **feature** | feature_flags, application_service, module | `feature/mod.rs` | ~15 |

#### synapse-services 域分组（6 组）

| 域 | 包含模块 | 域文件 | 类型数 |
|----|----------|--------|--------|
| **admin** | admin_audit_service ~ admin_user_service (8 个) | `admin.rs` ✅ 已完成 | ~20 |
| **room** | room::service, room::space, room::summary, directory_service, typing_service | `room/mod.rs` (已存在) | ~20 |
| **sync** | sync_service, sliding_sync_service, sync_helpers | `sync/mod.rs` | ~25 |
| **media** | media, media_service, media_quota_service, content_scanner | `media/mod.rs` | ~10 |
| **account** | account_data_service, account_device_list_service, account_identity_service, refresh_token_service, user_service | `account/mod.rs` | ~10 |
| **federation** | federation_blacklist_service, federation_key_rotation_service | `federation/mod.rs` | ~5 |

### 2.4 三层导出机制

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 1: 模块声明（lib.rs）                                      │
│  pub mod admin;   // 域分组模块                                  │
│  pub mod room;    // 已有                                        │
│  pub mod rtc;     // 已有                                        │
│  // 不再有扁平的 pub mod admin_audit_service;                    │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: 域 re-export（admin/mod.rs）                            │
│  pub use crate::admin_audit_service::AdminAuditService;         │
│  pub use crate::admin_federation_service::{...};                │
│  // 消费者可用: synapse_services::admin::AdminAuditService      │
├─────────────────────────────────────────────────────────────────┤
│ Layer 3: 兼容 prelude（prelude.rs）                              │
│  pub use crate::admin::*;                                       │
│  pub use crate::room::service::*;                               │
│  // 消费者可用: synapse_services::AdminAuditService (旧路径)    │
│  // 或: use synapse_services::prelude::*; (glob 导入)          │
└─────────────────────────────────────────────────────────────────┘
```

---

## 三、实施细节

### 3.1 渐进式实施路线图

| 阶段 | 内容 | 风险 | 涉及文件 |
|------|------|------|----------|
| **P0** ✅ | admin 域分组（services） | 低 | `admin.rs`（已完成） |
| **P1** | admin 域分组（storage） | 低 | 新建 `storage/admin/mod.rs` |
| **P2** | e2ee 域分组（storage） | 低 | 新建 `storage/e2ee/mod.rs` |
| **P3** | room 域分组（storage） | 中 | 新建 `storage/room/mod.rs`（可能与现有 room 冲突） |
| **P4** | auth 域分组（storage） | 中 | 新建 `storage/auth/mod.rs` |
| **P5** | sync 域分组（services） | 中 | 新建 `services/sync/mod.rs` |
| **P6** | prelude 模块 | 低 | 新建 `prelude.rs` |
| **P7** | 移除 lib.rs 扁平 re-export | 高 | 需更新所有消费者 import |

### 3.2 每个阶段的标准操作

以 P1（storage admin 域）为例：

**步骤 1**：创建 `synapse-storage/src/admin/mod.rs`

```rust
//! Admin storage domain group.
//!
//! Re-exports admin-related storage modules under a single namespace.
//! Consumers should prefer `synapse_storage::admin::AdminFederationStorage`
//! over the flat `synapse_storage::AdminFederationStorage`.

pub use crate::admin_federation::{
    AdminFederationStorage, AdminFederationStoreApi, FederationCacheRecord,
    FederationDestinationRecord, PendingFederationRecord,
};
pub use crate::admin_media::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage,
    AdminMediaQuotaSummary, AdminMediaStorage, AdminMediaStoreApi, MediaCursor,
};
pub use crate::audit::{
    decode_audit_event_cursor, encode_audit_event_cursor, AuditEvent,
    AuditEventCursor, AuditEventFilters, AuditEventStorage, AuditEventStoreApi,
    CreateAuditEventRequest,
};
```

**步骤 2**：在 lib.rs 中添加域模块声明

```rust
/// Admin storage domain group.
pub mod admin;
```

**步骤 3**：将 lib.rs 中的 admin re-export 替换为兼容 glob

```rust
// Before (lib.rs):
pub use self::admin_federation::{...};  // 5 行
pub use self::admin_media::{...};       // 4 行
pub use self::audit::{...};             // 5 行

// After (lib.rs):
pub use admin::*;  // 1 行，向后兼容
```

**步骤 4**：验证

```bash
cargo clippy --workspace --all-features --locked -- -D warnings
cargo test -p synapse-storage --lib
cargo test --all-features --test unit
```

### 3.3 命名冲突处理

当域分组导致命名冲突时（`ambiguous_glob_reexports`）：

```rust
// 方案 A: 在域模块中显式排除冲突项
pub use crate::admin_federation::{AdminFederationStorage, ...};
// 不 re-export 冲突的类型，消费者用完整路径

// 方案 B: 使用 #[allow(ambiguous_glob_reexports)]
#[allow(ambiguous_glob_reexports)]
pub use admin::*;
```

### 3.4 文档展示控制

使用 `#[doc(inline)]` 和 `#[doc(no_inline)]` 控制 rustdoc 展示：

```rust
// 域模块内的类型在文档中内联展示（消费者看到类型定义）
#[doc(inline)]
pub use crate::admin_audit_service::AdminAuditService;

// 兼容 prelude 的 re-export 不内联（避免文档重复）
#[doc(no_inline)]
pub use crate::admin::*;
```

---

## 四、测试验证

### 4.1 编译验证

```bash
# 1. 生产代码编译（无警告）
cargo build --workspace --all-features --locked

# 2. Clippy 严格检查
cargo clippy --workspace --all-features --locked -- -D warnings

# 3. 格式检查
cargo fmt --all -- --check

# 4. 测试代码编译
cargo test --workspace --all-features --locked --no-run
```

### 4.2 功能测试

```bash
# 单元测试
cargo test --all-features --test unit

# E2EE 库测试
cargo test -p synapse-e2ee --all-features --lib

# Services 库测试
cargo test -p synapse-services --all-features --lib

# Storage 库测试（需要数据库）
cargo test -p synapse-storage --all-features --lib
```

### 4.3 API 兼容性验证

验证旧路径和新路径都能工作：

```rust
// 旧路径（向后兼容）
use synapse_storage::UserStorage;

// 新路径（域分组）
use synapse_storage::auth::UserStorage;

// Prelude glob
use synapse_storage::prelude::*;
```

### 4.4 性能验证

重构是纯编译期变化（re-export 不影响运行时），但仍需验证：

```bash
# 编译时间不应显著增加
time cargo build --workspace --all-features --locked --release

# 二进制大小不应显著变化
ls -la target/release/synapse-rust
```

---

## 五、风险评估

### 5.1 风险矩阵

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| 命名冲突（ambiguous_glob_reexports） | 中 | 低 | 用 `#[allow]` 或显式排除 |
| 消费者 import 路径变化 | 低 | 中 | 保留 `pub use domain::*` 兼容 |
| 域分组边界判断错误 | 中 | 中 | 遵循 ServiceContainer 已有架构 |
| 编译时间增加 | 低 | 低 | re-export 不增加运行时开销 |
| 文档展示混乱 | 中 | 低 | 用 `#[doc(inline/no_inline)]` 控制 |

### 5.2 回滚策略

每个阶段都是独立的 git commit，可单独回滚：

```bash
# 回滚特定阶段
git revert <commit_hash>

# 回滚到重构前
git reset --hard <pre_refactor_commit>
```

**关键原则**：每个阶段完成后必须通过全部测试，不累积技术债。

---

## 六、重构前后对比

### 6.1 lib.rs 行数对比

```
synapse-storage/src/lib.rs:
  重构前: 562 行 (75 pub mod + 61 pub use + Database struct + tests)
  重构后: ~150 行 (8 pub mod[域] + 1 pub use prelude::* + Database struct + tests)
  减少:   ~73%

synapse-services/src/lib.rs:
  重构前: 283 行 (76 pub mod + 30 pub use)
  重构后: ~100 行 (6 pub mod[域] + 1 pub use prelude::*)
  减少:   ~65%
```

### 6.2 模块结构对比

**重构前（扁平）**：
```
synapse_storage::
├── UserStorage          ← 从哪个域来的？
├── DeviceStorage        ← 从哪个域来的？
├── AdminFederationStorage
├── AdminMediaStorage
├── AuditEventStorage
├── RoomStorage
├── MediaStorage
├── ... (61 个扁平类型)
```

**重构后（域分组）**：
```
synapse_storage::
├── admin::
│   ├── AdminFederationStorage   ← 清晰归属
│   ├── AdminMediaStorage
│   └── AuditEventStorage
├── auth::
│   ├── UserStorage              ← 清晰归属
│   ├── DeviceStorage
│   └── TokenStorage
├── room::
│   ├── RoomStorage
│   └── MembershipStorage
├── media::
│   └── MediaStorage
├── ... (8 个域)
└── prelude::*                   ← 向后兼容的扁平导入
```

### 6.3 新增模块流程对比

**重构前**：新增 `notification_storage` 模块
```
1. 创建 src/notification.rs
2. 编辑 lib.rs 添加 pub mod notification;
3. 编辑 lib.rs 添加 pub use notification::{NotificationStorage, ...};
4. lib.rs 又变长了
```

**重构后**：新增 `notification_storage` 模块
```
1. 创建 src/notification.rs
2. 编辑 sync/mod.rs 添加 pub use crate::notification::*;
3. lib.rs 不变
```

---

## 七、结论

### 7.1 可行性评估

| 维度 | 评估 | 说明 |
|------|------|------|
| Rust 2024 兼容性 | ✅ | 模块系统未变，方案基于稳定的 Rust 2018+ 特性 |
| 向后兼容 | ✅ | `pub use domain::*` 保持旧 import 路径可用 |
| 性能影响 | ✅ 无 | re-export 是编译期操作，不影响运行时 |
| 维护性提升 | ✅ 显著 | lib.rs 行数减少 65-73%，新增模块不需编辑 lib.rs |
| 测试覆盖 | ✅ | 现有 877 unit + 1506 services + 318 e2ee 测试验证 |

### 7.2 推荐实施顺序

1. **立即执行**（P1-P2）：admin + e2ee 域分组（低风险，模式已验证）
2. **短期执行**（P3-P4）：room + auth 域分组（中风险，需处理命名冲突）
3. **中期执行**（P5-P6）：sync 域 + prelude 模块
4. **长期评估**（P7）：移除 lib.rs 扁平 re-export（高风险，需全量更新消费者）

### 7.3 已完成进度

- ✅ P0：services admin 域分组（`admin.rs`，commit `95a582e2`）
- ⬜ P1-P7：待实施
