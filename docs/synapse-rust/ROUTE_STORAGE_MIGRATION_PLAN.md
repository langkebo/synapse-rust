# route → service → storage 迁移计划

> 起始基线: `scripts/ci/route_storage_exceptions.txt`（2026-06-03 快照，共 29 个文件、137 处 `use crate::storage::*`）
> 当前进度（2026-06-03 更新）：✅ **allowlist 已从 29 → 0（全部清空）**
> 目标: ✅ 已达成 — 路由层 0 直接 `crate::storage` 引用，CI 门禁 `check_route_storage_boundary.sh` 持续拦截
> 关联: [COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md](./COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) C-4
> 门禁: `scripts/ci/check_route_storage_boundary.sh`（每 PR 必跑）

## 一、迁移原则

1. **路由层职责单一**：解析参数 → 调用 service → 序列化响应。
2. **不允许在路由中出现 `use crate::storage`**：所有数据访问必须经过 service。
3. **service 层补齐缺失的方法**：发现 storage 调用未对应到 service 时，同步在 service 中添加薄包装。
4. **不允许借道 service 透传 `PgPool`**：service 只能暴露语义化方法（如 `find_user_by_id`），不能把池或 row 暴露给路由。
5. **批量迁移要保留事务边界**：当一个路由用到了多步 DB 操作，service 暴露 `txn` 方法而不是让路由自己做事务。
6. **每迁移一个文件，新增/更新单测**。

## 二、分批计划

### Batch 1（2026-06-10 前）— Auth / Admin 域（5 文件）

> 目标：审计/认证/管理员路径。所有这些文件都是 P0 关联面，迁移收益最高。

| 文件 | 现状 | 目标 service |
|---|---|---|
| `src/web/routes/auth_compat.rs` | 直连 `UserStorage`/`DeviceStorage`/`TokenStorage` | `services::auth::*`（已有 `LoginService`/`RegisterService`，需补 compat 专用） |
| `src/web/routes/guest.rs` | 直连 `UserStorage` | `services::auth::GuestService`（新建） |
| `src/web/routes/admin/audit.rs` | 直连 `AuditStorage` | `services::admin_audit_service`（已存在，需补 export/查询方法） |
| `src/web/routes/admin/federation.rs` | 直连 `FederationStorage` | `services::federation::*`（需新增） |
| `src/web/routes/admin/notification.rs` | 直连 `NotificationStorage` | `services::server_notification_service`（已存在） |

#### 步骤
1. `auth_compat.rs`: 在 `services/auth/compat.rs` 新增 `CompatAuthService`，聚合 login/register/refresh 三个老接口。
2. `guest.rs`: 抽取 `GuestService`。
3. `admin/*`: 三文件一一对应到现有 service，缺失的 federation 路径新建 `services::federation::FederationControlService`。
4. 删除 5 行 allowlist。
5. 更新单测：每个 service 新增 ≥ 2 个单测；每个路由保留 happy-path 集成测试。

#### 验收
- `bash scripts/ci/check_route_storage_boundary.sh` 输出 OK 且 allowlist 行数从 29 降至 24。
- `cargo test --lib services::auth::` 全绿。
- `cargo test --tests` 全部 admin/guest/compat 用例通过。

### Batch 2（2026-06-20 前）— Cross-cutting 域（8 文件）

> 目标：跨域横切关注点。

| 文件 | service |
|---|---|
| `src/web/routes/app_service.rs` | `services::application_service` |
| `src/web/routes/background_update.rs` | `services::background_update_service` |
| `src/web/routes/cas.rs` | `services::cas_service`（已存在） |
| `src/web/routes/event_report.rs` | `services::event_report_service` |
| `src/web/routes/feature_flags.rs` | `services::feature_flag_service` |
| `src/web/routes/extractors/auth.rs` | `web::middleware::auth`（提取器，本质上不是路由，但应迁出 storage 引用） |
| `src/web/routes/room_summary.rs` | `services::room::summary` |
| `src/web/routes/sliding_sync.rs` | `services::sliding_sync_service` |

#### 验收
- allowlist 行数从 24 降至 16。
- `cargo clippy` 与 `cargo test` 全绿。

### Batch 3（2026-07-01 前）— Push / Ext 域（6 文件）

| 文件 | service |
|---|---|
| `src/web/routes/push_notification.rs` | `services::push::service` |
| `src/web/routes/module.rs` | `services::module_service` |
| `src/web/routes/openclaw.rs` | `services::openclaw_service` |
| `src/web/routes/rendezvous.rs` | `services::rendezvous_service`（新建） |
| `src/web/routes/space/types.rs` | `services::room::space` |
| `src/web/routes/ai_connection.rs` | `services::matrix_ai_connection_service` |

#### 验收
- allowlist 行数从 16 降至 10。

### Batch 4（2026-07-15 前）— Admin/Users 收尾（5 文件）

| 文件 | service |
|---|---|
| `src/web/routes/admin/room.rs` | `services::room::service` |
| `src/web/routes/admin/token.rs` | `services::refresh_token_service` |
| `src/web/routes/admin/user.rs` | `services::registration_service` |
| `src/web/routes/handlers/search.rs` | `services::search_service` |
| `src/web/routes/handlers/thread.rs` | `services::thread_service` |

#### 验收
- allowlist 行数从 10 降至 5。

### Batch 5（2026-07-30 前）— Room handlers（5 文件，复杂度最高，留到最后）

| 文件 | service |
|---|---|
| `src/web/routes/handlers/room/events.rs` | `services::room::events`（新建，与 `services::sync_service` 共享查询） |
| `src/web/routes/handlers/room/members.rs` | `services::room::members` |
| `src/web/routes/handlers/room/mod.rs` | `services::room::Router`（门面） |
| `src/web/routes/handlers/room/receipts.rs` | `services::room::receipts` |
| `src/web/routes/handlers/room/state.rs` | `services::room::state` |

#### 验收
- allowlist 长度 = 0。
- `cargo clippy --all-features --locked -- -D warnings` 通过。
- `cargo test --tests --all-features --locked` 全绿。
- 集成测试 `/rooms/*` 全部通过。

## 三、迁移模板

每个文件按以下模板改造：

```rust
// BEFORE (route 直接 use storage)
use crate::storage::user::UserStorage;
use crate::storage::device::DeviceStorage;

pub async fn list_devices(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> ApiResult<Json<DevicesResponse>> {
    let pool = &state.services.pool;
    let devices = sqlx::query_as::<_, DeviceRow>("SELECT * FROM devices WHERE user_id = $1")
        .bind(&user.user_id)
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(DevicesResponse { devices }))
}

// AFTER (route 仅依赖 service)
use crate::services::device::DeviceService;

pub async fn list_devices(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> ApiResult<Json<DevicesResponse>> {
    let devices = state.services.device_service.list_for_user(&user.user_id).await?;
    Ok(Json(DevicesResponse { devices }))
}
```

## 四、回归保障

- 每个 batch 完成后：
  1. `bash scripts/ci/check_route_storage_boundary.sh` 通过且行数符合预期。
  2. `cargo test --all-features --locked` 全绿。
  3. PR 标题以 `refactor(routes):` 开头，便于回溯。
- 任何 batch 中发现 service 缺失的方法，先在 service 中实现并补单测，再在路由里使用。

## 五、追踪看板

每完成一个文件，将对应行从 `scripts/ci/route_storage_exceptions.txt` 删除并在 PR 描述中标注 allowlist delta（如 `allowlist: 29 → 28`）。
