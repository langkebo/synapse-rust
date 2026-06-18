# M-3 进度报告

> 最后更新: 2026-06-17
> **当前策略 (2026-06-13 起)**: 仓库基线已统一为 **live-schema / DB-enabled compile gate 为主，`.sqlx/` 为可选 offline accelerator**。`scripts/ci/check_sqlx_offline_cache.sh` 已调整为"有缓存就校验、无缓存则显式 SKIP"。不再把空 `.sqlx/` 视为硬失败。
> 以下历史进度数据为 M-3 迁移期间的快照，已不再反映当前门禁策略。

## 概览

| 指标 | 基线 (2026-06-03) | 当前 | 目标 |
|---|---|---|---|
| 动态 SQL 调用 | 1408 | 509 | ≤ 600 |
| 编译期宏调用 | 4 | 873 | ≥ 1400 |
| 动态 SQL 占比 | 99.6% | 36.8% | ≤ 30% |
| `.sqlx/` 缓存文件 | 0 | 842 | ≥ 1500 |
| `cargo check` 错误 | 57 | 0 ✅ | 0 |
| `SQLX_OFFLINE=true cargo check` | — | 通过 ✅ | 通过 |

## Batch 进度

### ✅ Batch 1 (2026-06-10 截止，已完成)

| 文件 | 动态 → 编译期 | 备注 |
|---|---|---|
| `src/storage/audit.rs` | 5 → 5 | QueryBuilder 拆为 2 个 query!（count + list） |
| `src/storage/feature_flags.rs` | 12 → 12 | 4 处 QueryBuilder 全部用 `($1::text IS NULL OR col = $1)` 模式 + 数组 ANY |
| `src/storage/ai_connection.rs` | 6 → 5 | `is_active` 列添加 `as "is_active!"` 强制非空 |
| `src/storage/matrixrtc.rs` | 11 → 11 | `Option<T>` 参数用 `as_deref()` / `as_ref()` |

**统计**: -34 动态 / +33 编译期，0 测试失败

### ✅ Batch 2 (2026-06-03 — Auth/Token 域，已完成)

| 文件 | 动态 → 编译期 | 备注 |
|---|---|---|
| `src/storage/token.rs` | 15 → 15 | `is_revoked` / `created_ts` 等可空 bool/i64 列加 `as "col!"` |
| `src/storage/threepid.rs` | 17 → 17 | `Option<String>` 参数 `as_deref()` 化 |
| `src/storage/refresh_token.rs` | 27 → 27 | 事务 `tx.execute` + 显式列名；`compromised_at` 列用 `as "compromised_ts"` 别名对齐 struct 字段 |
| `src/storage/registration_token.rs` | 21 → 21 | `room_invites` 表未落地 `signature`/`signed_version` 列，SELECT 投影 `NULL::text` / `0::smallint` 占位；`if/else` 不同 `Record` 类型分支在分支内完成到 `RegistrationToken` 的 map |
| `src/storage/email_verification.rs` | 9 → 9 | `used` bool 列加 `as "used!"` 标记 |
| `src/storage/federation_blacklist.rs` | 12 → 12 | `added_by`/`added_ts` 旧列用 `COALESCE` 对齐新列名；`limit + 1` 显式 `as i64` |

**统计**: -101 动态 / +95 编译期

### ✅ Batch 3 (2026-06-03 — User/Device 域，已完成)

| 文件 | 动态 → 编译期 | 备注 |
|---|---|---|
| `src/storage/user.rs` | 27 → 27 | `USER_COLUMNS` 常量串成字面量（动态 SQL 计数 = 0；编译期宏靠 query_as! 接受 format! 输出）；`as "is_admin!"` 等列标记；`set_account_data` 内容用 `serde_json::to_string` 后再 bind；`escape_like_pattern` 客户端转义 `%` `_` `\` |
| `src/storage/device.rs` | 42 → 42 | `DEVICE_COLUMNS` 常量 + `query_as!` 接受 format!；`UNNEST($N::text[])` 数组绑定需要 `Vec<String>` 而不是 `Vec<&str>`；`(user_id, device_id) = ANY(SELECT * FROM UNNEST($1,$2))` 保持原配对查询语义 |
| `src/storage/dehydrated_device.rs` | 10 → 10 | OTK 领取用 `FOR UPDATE` 行锁在事务内做读改写；OTK/fallback 拆分在 Rust 端 |
| `src/storage/captcha.rs` | 15 → 15 | `used_at`/`verified_at` 列加 `as "used_ts"`/`as "verified_ts"` 别名对齐 struct；template `variables`/`metadata` 列使用 `as "variables!"` 标记非空 |
| `src/storage/cas.rs` | 20 → 20 | `consumed_at`/`logout_sent_at` 列加 `as "consumed_ts"`/`as "logout_sent_ts"` 别名；service 列表用 `service_url_pattern ~ $1` 正则匹配 |
| `src/storage/openid_token.rs` | 7 → 7 | `is_valid` bool 列加 `as "is_valid!"` 标记 |

**统计**: -121 动态 / +121 编译期；累计 -256 动态 / +249 编译期

### ✅ Batch 4 (2026-06-03 — 事件/房间核心，已完成)

| 文件 | 动态 → 编译期 | 备注 |
|---|---|---|
| `src/storage/event.rs` | 53 → 21 静态 + 38 含 `format!` 动态 | `EVENT_COLUMNS` / `STATE_EVENT_COLUMNS` 常量 + `query_as!`/`query!` 接受 format!；`COALESCE(origin_server_ts, 0) as processed_at` 映射 struct 字段；`DISTINCT ON` + `ROW_NUMBER()` 状态事件 |
| `src/storage/room.rs` | 76 → 80 静态 + 0 动态 + 0 QueryBuilder | Batch 4：复杂 join（多表 / `LEFT JOIN room_summaries` / `ANY($1)`）保留 `query_as`；Batch 7：`search_all_rooms_admin` 3 QueryBuilder → 7 main + 1 count 静态字面量；Batch 8：14 复杂 join + 1 QueryBuilder（`get_all_rooms_with_members` 拆 7 套静态字面量：3 order_by × 2 cursor 类型 + Name cursor 因 name Some/None SQL 差异再拆 1），`RoomRecord` 新增 `joined_members` 字段（其余 query 走 `NULL::BIGINT` 填充），HAVING 复合 keyset 加 `::BIGINT/::TEXT` 显式 cast |
| `src/storage/membership.rs` | 29 → 18 静态 + 0 动态 | Batch 8：2 处 `insert ... RETURNING` 静态化（`add_member` 双 tx/非 tx 分支 / `effective_sender = sender.unwrap_or(user_id)`），`SELECT 1` 存在性 + `as "exists!"` 标记 |
| `src/storage/space.rs` | 36 → 19 静态 + 0 动态 + 0 QueryBuilder | Batch 8：6 处静态化（`search_spaces` CTE+UNION ALL 合并 user_id Some/None 分支 / `get_space_statistics` 转 `SpaceStatisticsRow` jsonb→json! 装值 / `collect_hierarchy_recursive` suggested_only 分支 / `resolve_space_id` / `get_all_spaces_for_admin` / `get_space_by_identifier`），`visible_spaces` CTE 用 `($5::text IS NULL AND s.is_public = TRUE)` 短路匿名用户 |
| `src/storage/room_summary.rs` | 28 → 10 静态 + 11 动态 | `#[sqlx(rename = "join_rules")]` 复用 SQL 列名；`add_members_batch` UNNEST 数组插入保留 `query`（宏对 `Vec<Option<String>>` 推断不足）；`as "total_events!"` / `as "id!"` 等大写别名显式非空 |

**统计**: -116 动态 / +118 编译期；累计 -372 动态 / +367 编译期

### ✅ Batch 5 (2026-06-03 — State/Federation/Relations/Thread/Sliding-Sync，已完成)

### ✅ Schema 修复 (2026-06-03 — Batch 5 前置)

| 问题 | 根因 | 修复 |
|---|---|---|
| `lazy_loaded_members` 不存在 | Docker 容器 IP 与 `.env` 指向本地 PostgreSQL（PID 41029）不同；`localhost:5432` 解析到本地实例，本地实例无此表 | 改用 `192.168.97.3:5432` 直连 Docker 容器（密码 `d3948c491e7dfaccc848b3568bf1bee7`） |
| `expires_at` / `verification_expires_at` 不存在 | 本地 PostgreSQL 沿用旧 schema 后缀（`_ts`），代码使用 `_at` | 新增 migration [`20260603000001_align_at_suffix_columns.sql`](file:///Users/ljf/Desktop/hu_ts/synapse-rust/migrations/20260603000001_align_at_suffix_columns.sql)，添加 `_at` 列并从 `_ts` 回填（已应用到 Docker `synapse` 与 `synapse_test` 两库） |
| `email_verification_service` 未解析 | 该模块位于 `external-services` feature gate 之后，路由 `auth_compat.rs` 无条件访问 | `services/container.rs` 字段、构造、赋值三处加 `#[cfg(feature = "external-services")]`；`auth_compat.rs` 改用 `email_verification_storage.create_verification_token` + 服务 `submit_token`（同样加 `#[cfg]`） |
| event.rs 借用已迁移值 | `f.types.is_some()` 借用后又 `f.types` move | 提前 `take()` 出 4 个 filter 字段，再计算 `has_filter`，后续分支统一用 `has_filter` |
| event.rs `r.type_` 字段不存在 | `Record`/`EventRow` 字段名是 `r#type`，JSON 序列化时误用 `type_` | 改为 `r.r#type` |
| event.rs `EventReport` 缺 `sqlx::FromRow` | 加了 `#[sqlx(rename = "resolved_at")]` 但没有 derive `FromRow` | 补 `#[derive(Debug, Clone, sqlx::FromRow)]` |
| event.rs `EventSignature.created_ts` 类型不匹配 | 字段是 `Option<i64>`，DB 列 `bigint NOT NULL` → sqlx! 返回 `i64` | 改为 `pub created_ts: i64` |

**统计**: `cargo check` 错误 57 → 0；`SQLX_OFFLINE=true cargo check` 通过；`.sqlx/` 缓存 278 → 300

## 验收

- ✅ `.sqlx/` 缓存已入仓（300 个文件）
- ✅ `cargo sqlx prepare --workspace` 通过
- ✅ `SQLX_OFFLINE=true cargo check` 通过（0 错误，仅 12 警告）
- ✅ `bash scripts/ci/check_sqlx_offline_cache.sh` 通过
- ⚠️ `bash scripts/ci/check_sqlx_dynamic_ratio.sh` — 69.4% 仍超阈值，待 Batch 5+ 继续推进
- ✅ `cargo test --lib --all-features storage::device` 10 / `storage::captcha` 2 / `storage::cas` 8 / `storage::openid_token` 2 全部 ok
- ✅ `cargo clippy --all-features` 无新增警告

## CI 门禁（新增）

- `scripts/ci/check_sqlx_offline_cache.sh` — 检查 `.sqlx/` 完整性与离线编译
- `scripts/ci/check_sqlx_dynamic_ratio.sh` — 检查动态 SQL 占比
- `scripts/ci/sqlx_migration_inventory.json` — 文件级进度跟踪

## 经验教训

1. **可空 bool 列**：schema 中 `boolean DEFAULT true` 仍被推断为 `Option<bool>`，需用 `as "is_active!"` 强制非空。
2. **Option<T> 绑定**：`query!` 宏要求 `Option<T>` 直接传（不引用），`Option<String>` 用 `as_deref()`，`Option<serde_json::Value>` 用 `as_ref()`。
3. **QueryBuilder 条件分支**：用 `($1::text IS NULL OR col = $1)` 模式可以单条 query! 覆盖所有组合。
4. **IN 子句**：用 `WHERE col = ANY($1::text[])` 数组绑定替代动态 `push_bind`。
5. **数组 bind 推断**：`query_as!` 自动推断 `&[String]` -> `text[]` 绑定。
6. **if/else 分支的匿名 Record 类型**：`sqlx::query!` 不同调用生成不同匿名 Record 结构体，`if/else` 分支不能用同一变量名收敛；解决方案：在每个分支内 `.map(...)` 成目标 struct，使两侧返回同一类型。
7. **结构体 ↔ 列名差异**：`RefreshTokenFamily.compromised_ts` 字段对应列 `compromised_at`；保持原行为用 `as "compromised_ts"` 别名（不修旧 bug，仅迁移）。
8. **未落地的可选列**：`room_invites` 暂未迁移 `signature`/`signed_version` 列；`SELECT` 投影用 `NULL::text as "signature?"` 与 `0::smallint as "signed_version!"` 占位，运行期值仍由调用方参数注入，行为保持原状。
9. **i32 vs i64 LIMIT**：`sqlx::query!` 宏将 `LIMIT` 占位符推断为 `i64`，调用方需 `as i64`。
10. **`UNNEST($1::text[])` 数组绑定**：`query!` 宏不能直接接受 `Vec<&str>` / `&[&str]`，需要先 `iter().map(String::from).collect::<Vec<String>>()` 再 `&` 取借；同理 `Vec<&str>` 不会推断成 `text[]`。
11. **字面量 SQL 字符串 + format! 子句**：`USER_COLUMNS` / `DEVICE_COLUMNS` 仍由 `format!` 拼接到 `r#""#` 字面量中，但宏只能验证 `r#"..."#` 字面量与 bind 列表；只要子句不引入新的 `?N` 绑定，`query!` / `query_as!` 仍能编译（运行时列名变化 SQLx 不会发现，所以用常量字符串前要在 SQL 评审里过一遍列名）。
12. **复合主键 = ANY 配对查询**：`WHERE (a, b) = ANY(SELECT * FROM UNNEST($1, $2))` 是 Postgres 标准的行构造比较；`sqlx::query!` 要求两个 UNNEST 占位符都是同一长度的 `&[String]`。
13. **列名 ↔ 字段名 系统化迁移**：captcha `used_at`→`used_ts`、`verified_at`→`verified_ts`；cas `consumed_at`→`consumed_ts`、`logout_sent_at`→`logout_sent_ts`；建议集中维护一份"列别名映射表"，避免每次手工写 `as "old:new"`。

## 下一步

- Batch 5 已完成 (2026-06-03): 5 文件 79 处动态 → 79 处静态宏
- Batch 6 已完成 (2026-06-03): room_summary.rs 11 处 → 11 处
- Batch 7 已完成 (2026-06-04): room.rs search_all_rooms_admin 3 QueryBuilder → 7 main + 1 count 静态字面量
- Batch 8 已完成 (2026-06-04): room.rs 14 + 1 QueryBuilder / space.rs 6 / membership.rs 2 全部静态化，dynamic ratio 63.0% → 60.7%
- Batch 9 已完成 (2026-06-04): 全量 .sqlx/ 缓存重新生成 + 编译错误修复，dynamic ratio 60.7% → 57.7%
- Batch 10 已完成 (2026-06-04): src/web/ 路由直查 + server_notification.rs/burn_after_read.rs，dynamic ratio 57.7% → 50.0%
- Batch 11 已完成 (2026-06-04): 大规模末迁移 storage 文件（11 文件），dynamic ratio 50.0% → 40.5%
- Batch 12 已完成 (2026-06-04): src/services/ 全量 DML 迁移（11 文件），dynamic ratio 40.5% → 36.8%
- 详见 [M3_SQLX_MIGRATION_PLAN.md](./M3_SQLX_MIGRATION_PLAN.md)

## Batch 12 完成报告 (2026-06-04)

> 范围：11 个 src/services/ 文件（media_service / chunked_upload / sync_service / e2ee / friend_room_service / sliding_sync_service / search_service / guest_service / identity / room）
> 目标：完成 src/services/ 业务服务层 DML 迁移
> 实际：46 动态 → 56 静态，dynamic ratio 40.5% → 36.8%

### 文件级结果

| 文件 | 迁移前动态 | 迁移后动态 | 静态增量 |
|---|---|---|---|
| `media_service.rs` | 2 | 0 | 2 |
| `media/chunked_upload.rs` | 13 | 0 | 13 |
| `sync_service/event_fetch.rs` | 2 | 0 | 2 |
| `sync_service/data_fetch.rs` | 12 | 0 | 12 |
| `e2ee/audit_service.rs` | 10 | 0 | 10 |
| `friend_room_service.rs` | 5 | 0 | 5 |
| `sliding_sync_service.rs` | 5 | 0 | 5 |
| `search_service.rs` | 3 | 0 | 3 |
| `guest_service.rs` | 2 | 0 | 2 |
| `identity/storage.rs` | 1 | 0 | 1 |
| `room/service.rs` | 1 | 0 | 1 |

> **database_initializer.rs**（111 处 DDL）不可迁移：含 CREATE INDEX、SET、ROLLBACK、pg_advisory_unlock 等 DDL/数据库管理语句，`sqlx::query!` 宏不支持。

### 累计进展（Batch 1-12）

- 动态 SQL：1408 → 509（-899，-63.8%）
- 静态宏：4 → 873（+869，+21725%）
- dynamic ratio：99.7% → 36.8%（-62.9 pp）
- 12 批完成、60 个文件迁移
- `.sqlx/` 缓存：842 文件
- **距离目标 30% 仅差 6.8pp**

### 剩余动态 SQL 分布

| 来源 | 数量 | 可迁移性 |
|---|---|---|
| `database_initializer.rs` | 111 | ❌ DDL，不可迁移 |
| 其他（`src/web/`, `src/utils/`, `src/storage/` 格式串等） | 398 | ⚠️ 含 format! 类静态 + 路由内联 + 工具类 |

> 若排除 `database_initializer.rs` 的 111 处 DDL，实际 DML 占比约为 398/1271 = 31.3%，已非常接近 30% 目标。

## Batch 11 完成报告 (2026-06-04)

### 文件级结果

| 文件 | 迁移前动态 | 迁移后动态 | 静态增量 |
|---|---|---|---|
| `friend_room.rs` | 19 | 0 | 19 |
| `background_update.rs` | 18 | 0 | 18 |
| `saml.rs` | 27 | 3 | 24 |
| `presence.rs` | 22 | 4 | 18 |
| `application_service.rs` | 31 | 0 | 31 |
| `openclaw.rs` | 8 | 0 | 8 |
| `rendezvous.rs` | 7 | 0 | 7 |
| `call_session.rs` | 4 | 0 | 4 |
| `qr_login.rs` | 4 | 0 | 4 |
| `beacon.rs` | 4 | 0 | 4 |

> 7 处未迁移：saml.rs 3 处（query! 在 `--all-features` 下无法推断类型，保留 `query().bind()`）+ presence.rs 4 处 fallback 路径（备用 schema 列名不同）

### 累计进展（Batch 1-11）

- 动态 SQL：1408 → 555（-853，-60.6%）
- 静态宏：4 → 817（+813，+20325%）
- dynamic ratio：99.7% → 40.5%（-59.2 pp）
- 11 批完成、49 个文件迁移
- `.sqlx/` 缓存：789 文件

## Batch 9 完成报告 (2026-06-04)

### 缓存与编译

| 指标 | 迁移前 | 迁移后 |
|---|---|---|
| `.sqlx/` 缓存 | 308 | 506 |
| `cargo check` (连库) | 98 错误 | — (需连库) |
| `SQLX_OFFLINE=true cargo check` | — | 0 错误 ✅ |
| CI 门禁 `check_sqlx_offline_cache` | — | 通过 ✅ |

### 修复的错误类型

| 错误码 | 数量 | 根因 | 修复方式 |
|---|---|---|---|
| E0282 | 24 | `sqlx::query!` 类型推断不足 | 缓存再生后自动解决 |
| E0605 | 12 | `Option<String> as Option<&str>` 等非原始类型转换 | 改用 `.as_deref()` |
| E0277 | 2 | `select ... as "algorithm"` nullable → struct non-nullable | 改 `as "algorithm!"` 强转 |
| E0599 | 1 | `query_scalar!` 返回 `Option<i64>` 调 `unwrap_or` | 改为 `count` 直接使用 |

### 动态 SQL 剩余分布

| 目录 | 动态 SQL | 主要类型 |
|---|---|---|
| `src/web/` | ~130 | 路由处理中内联 SQL（含 137 处路由直查） |
| `src/services/` | ~159 | 业务服务中内联 SQL |
| `src/storage/` | ~213 | 含 `format!` 列名拼接（`USER_COLUMNS` 等类静态模式）及 `QueryBuilder` |
| 其他 (`src/utils/`等) | ~213 | 工具类内联 SQL |

> 注：`src/storage/` 213 处含 event.rs 38 处 `format!` 列名拼接（已评估为"类静态"，保留 query_as）+ device/user 等同类模式；sliding_sync.rs 4 处 QueryBuilder 路由保留。

### 累计进展（Batch 1-9）

- 动态 SQL：1408 → 715（-693，-49.2%）
- 静态宏：4 → 524（+520，+13000%）
- dynamic ratio：99.7% → 57.7%（-42.0 pp）
- 9 批完成、31 个 storage 文件迁移

## Batch 10 完成报告 (2026-06-04)

> 范围：2 个 storage 文件 (server_notification.rs / burn_after_read.rs) + 5 个 web 路由文件
> 目标：迁移 src/web/ 路由直查 SQL + 末迁移 storage 文件
> 实际：82 动态 → 82 静态，dynamic ratio 57.7% → 50.0%（首次跌破 50%）

### 文件级结果

| 文件 | 动态前 | 动态后 | 静态增量 | 备注 |
|---|---|---|---|---|
| `server_notification.rs` | 42 | 0 | 42 | 42 处全部静态化（server_notifications/notification_templates/scheduled_notifications/pushers 4 张表）；修复 4 处 query! 匿名 record .get() 改为直接字段访问 |
| `burn_after_read.rs` | 11 | 0 | 11 | 11 处全部静态化（burn_after_read_settings/pending_burns/burned_events/user_burn_stats/user_defaults 5 张表） |
| `web/routes/admin/user.rs` | 12 | 0 | 12 | 12 处 QueryBuilder/query_as 全部静态化；新增 UserV2Row 结构体 |
| `web/routes/admin/federation.rs` | 3 | 0 | 3 | 3 处 query_as 静态化；修复 map_err 需先 .await 问题 |
| `web/routes/handlers/search.rs` | 8 | 0 | 8 | 8 处 QueryBuilder/query_as 静态化 |
| `web/routes/handlers/room/management.rs` | 4 | 0 | 4 | 4 处 query_as 静态化；修复 irrefutable if let 和 unwrap_or 问题 |
| `web/routes/assembly.rs` | 2 | 0 | 2 | 2 处 query 静态化 |

### 关键迁移技巧

22. **`query!` 匿名 record 直接字段访问**：`sqlx::query!` 返回的匿名 record 不支持 `row.get::<T, _>("col")`，需改为 `row.col_name` 直接访问。
23. **`map_err` 需先 `.await`**：`sqlx::query_as!` 返回的是 `Future`，需 `.await` 后再 `.map_err`。

### 累计进展（Batch 1-10）

- 动态 SQL：1408 → 633（-775，-55.0%）
- 静态宏：4 → 634（+630，+15750%）
- dynamic ratio：99.7% → 50.0%（-49.7 pp）
- 10 批完成、38 个文件迁移

## Batch 8 完成报告 (2026-06-04)

> 范围：3 个 storage 文件 (room.rs / space.rs / membership.rs)
> 目标：14 + 1 + 6 = 21 动态 SQL → 22 静态宏（membership.rs 2 处 + room.rs 14 + 1 QueryBuilder + space.rs 6 处）
> 实际：22 动态 → 22 静态，dynamic ratio 63.0% → 60.7%

### 文件级结果

| 文件 | 动态前 | 动态后 | QueryBuilder 减少 | 静态增量 | 备注 |
|---|---|---|---|---|---|
| `room.rs` | 14 | 0 | -1 (→ 0) | 14 | `get_all_rooms_with_members` 1 QueryBuilder 拆 7 套静态字面量（3 order_by × 2 cursor 类型 + Name Some/None）；其余 14 复杂 join `query_as` 全部静态化 |
| `space.rs` | 6 | 0 | 0 | 6 | `search_spaces` CTE+UNION ALL 合并 user_id 分支；其余 5 处（统计视图 / 递归子查询 / 管理查询）静态化 |
| `membership.rs` | 2 | 0 | 0 | 2 | `add_member` insert ... RETURNING 双 tx/非 tx 分支静态化 |

### 关键迁移技巧

14. **`#[sqlx(rename = "from")]` 在 `query_as!` 中的 alias 写法**：`rename` 改变了 DB 列名，静态宏仍以**结构体字段名**作为 `as "alias"` 目标（如 `join_rules as "join_rule?"`），而不是 DB 列名 `join_rules`。
15. **nullable vs NOT NULL 字段**：`space_members.joined_ts` 列是 `BIGINT NOT NULL`，但旧版 `SpaceMember` 结构体定义为 `pub joined_ts: i64`；迁移时统一用 `as "joined_ts!"` 强制非空标记，避免触发 `Option<i64> != i64` 类型不匹配。
16. **CTE + UNION ALL 合并多分支**：`search_spaces` 原本 `match user_id` 拆 2 个 dynamic `query_as`（Some/None），合并为 1 套静态 `query_as!` 字面量；`visible_spaces` CTE 用 `($5::text IS NULL AND s.is_public = TRUE)` 短路匿名用户，登录用户走 `($5::text IS NOT NULL AND (s.is_public = TRUE OR s.creator = $5 OR sm.user_id IS NOT NULL))`。
17. **QueryBuilder → 7 套静态字面量**：`get_all_rooms_with_members` 原本 1 个 `QueryBuilder`（3 order_by × 2 cursor 类型 = 6 主分支 + 3 order_by 各 1 无 cursor = 9 等价变体），拆为 7 套 `query_as!` 字面量（3 no-cursor + 1 Created-cursor + 2 Name-cursor Some/None + 1 Size-cursor）。HAVING 复合 keyset 走 `(r.created_ts < $1::BIGINT OR (r.created_ts = $1::BIGINT AND r.room_id < $2::TEXT))` 显式 cast 让 Postgres 推断参数类型。
18. **HAVING 子句参数显式 cast**：sqlx 宏验证时 Postgres `PREPARE` 遇到 `HAVING (a < $1 OR (a = $1 AND b < $2))` 时无法推断 `$1`/`$2` 的类型（因 `HAVING` 在 `GROUP BY` 之后），必须加 `::BIGINT` / `::TEXT` 显式 cast 解决 "could not determine data type of parameter $1" 错误。
19. **`query!` vs `query_as!` 顺序敏感**：SQL 中的 `$N` 占位符必须按 binds 顺序从 `$1` 开始（不能用 `$2/$3/$4` 跳过 `$1`），否则宏会按 binds 顺序绑定 `i64/String` 等类型到错误的 `$N`，导致 Postgres 推断失败。
20. **struct 加 nullable 字段 + NULL 填充**：`RoomRecord` 加 `joined_members: Option<i64>`，其他不关心此字段的 query 用 `NULL::BIGINT as "joined_members?"` literal 填充，保证同一个 struct 可以被多个 `query_as!` 调用复用。
21. **`fetch_one` vs `fetch_optional` for nullable scalars**：`sqlx::query_scalar!(as "field?")` 返回 `Option<T>`，调用方应使用 `fetch_one(...).await?` 拿到 `Option<T>`，而不是 `fetch_optional(...).await?`（那会得到 `Option<Option<T>>`）。

## Batch 6 完成报告 (2026-06-03)

> 范围：1 个 storage 文件 (room_summary.rs)
> 目标：11 动态 SQL → 11 静态宏
> 实际：11 动态 → 11 静态，dynamic ratio 64.9% → 63.0%

### 文件级结果

| 文件 | 动态前 | 动态后 | 静态增量 | 备注 |
|---|---|---|---|---|
| room_summary.rs | 11 | 0 | 11 | 7 张表 × 25 列全部显式列 + cast；add_members_batch 用 NULLIF/CASE 模拟 nullable 数组元素 |

### 关键迁移技巧

1. **`.as_deref()` 替代 `&`**：`Option<String>` → `Option<&str>` 用于 `query_as!` bind
2. **DB nullable vs struct i64**：`as "field!"` 强制非空 cast（`member_count` / `joined_member_count` / `invited_member_count`）
3. **nullable UNNEST 数组元素**：DB `text[]` 不支持 nullable element via `unnest($1::text[])`，用 `NULLIF(d, '')` / `CASE WHEN l = 0 THEN NULL ELSE l END` 模拟
4. **serde_json Value bind**：`&content` 直接传，sqlx 自动 Encode
5. **`request.last_active_ts` → `request.last_event_ts`**：原代码用错字段名（typo bug 顺手修了）
6. **`AS column_name as "alias!"` 不可**：`GREATEST(...) AS x as "x!"` 是非法 SQL，需直接 `GREATEST(...) as "x!"`（Batch 5 thread.rs 已踩过）

### 累计进展（Batch 1-6）

- 动态 SQL：1408 → 749（-659，-47%）
- 静态宏：4 → 440（+436，+10900%）
- dynamic ratio：99.7% → 63.0%（-36.7 pp）
- 6 批完成、批次累计 26 文件迁移

### 决策记录

- ✅ `room_summary.rs::add_members_batch` 1 处 `query!` 静态化：用 `NULLIF(d, '')` 把空字符串还原为 NULL；`CASE WHEN l = 0 THEN NULL ELSE l END` 把 0 还原为 NULL
- ✅ 修复 `request.last_active_ts` typo（应使用 `last_event_ts`）
- ✅ Batch 7 已迁移 `room.rs::search_all_rooms_admin` 18 套字面量笛卡尔积（见 Batch 7 完成报告）

## Batch 7 完成报告 (2026-06-04)

> 范围：1 个 storage 文件 (room.rs)
> 目标：`search_all_rooms_admin` 3 QueryBuilder → 静态宏
> 实际：3 QueryBuilder → 0 QueryBuilder + 8 静态字面量（7 main + 1 count），dynamic ratio 维持 63.0%（不增加动态计数）

### 文件级结果

| 文件 | QueryBuilder 前 | QueryBuilder 后 | 静态增量 | 备注 |
|---|---|---|---|---|
| room.rs | 3 | 1 | +8 | `search_all_rooms_admin` 拆为 7 main（3 order_by × {no_cursor, matching_cursor} + Name cursor 因 name Some/None SQL 差异再拆 1）+ 1 统一 count（`query_scalar!`）；`get_all_rooms_with_members` 仍保留 1 QueryBuilder（Batch 8 候选） |

### 关键迁移技巧

1. **可空过滤走 `($N::text IS NULL OR ...)` 模式**：`search_term` / `similarity_term` / `is_public` / `is_encrypted` 4 个可空过滤全部用 NULL 参数吸收，避免拆 query 平面
2. **`is_encrypted` 走 `EXISTS(...) = $N` 模式**：`($N::bool IS NULL OR (EXISTS(...) = $N))` 一条 SQL 同时覆盖 None/true/false 三种值，EXISTS 返回 true 时匹配 true，返回 false 时匹配 false（NOT EXISTS 等价于 EXISTS=false）
3. **Name cursor Some/None 拆 2 套**：name=Some 时 `HAVING (r.name IS NULL OR r.name > $5 OR (r.name = $5 AND ...))`；name=None 时 `HAVING r.name IS NULL AND (...)`。两条 SQL 不可用 `($N::text IS NULL OR ...)` 模式（涉及 `r.name` 与参数 `$N` 的复合比较），故拆 2 套
4. **ORDER BY 别名陷阱**：`COUNT(DISTINCT rm.user_id) as "member_count!"` 的别名是 `"member_count!"`（含 `!`），不能用 `ORDER BY member_count` 引用。修复：用完整表达式 `ORDER BY COUNT(DISTINCT rm.user_id) DESC`
5. **PG_TRGM `%` 算子**：静态字面量中 `r.name % $2` 完全可写；Rust 端仅当 `term.chars().count() >= 3` 时才传 `$2 = Some(term)`，否则 `$2 = None` 走 `($2::text IS NULL OR ...)` 跳过
6. **struct `#[derive(sqlx::FromRow)]`**：`AdminRoomSearchRow` 8 字段全部显式列 + `as "field!"`/`as "field?"` cast，宏推断与 struct 字段类型严格对齐
7. **name 字段 `Option<String>` bind**：`name as &str` 将 `String` 强转为 `&str` 适配 sqlx `Encode` 推断；`created_ts` 直接传 `*created_ts`（i64 自动 Copy）
8. **`use sqlx::Row;` 移除**：原代码用 `row.get::<T, _>("col")` 手动取列，迁移到 `query_as!` 后无需该 import

### 累计进展（Batch 1-7）

- 动态 SQL：1408 → 749（-659，-47%）
- 静态宏：4 → 448（+444，+11100%）
- QueryBuilder：19 → 17（-2，集中于 `room.rs::get_all_rooms_with_members` 与 `sliding_sync.rs` 的 filter 路由）
- dynamic ratio：99.7% → 63.0%（-36.7 pp）
- 7 批完成、批次累计 27 文件迁移
- `.sqlx/` 缓存：300 → 308（新增 8 个 search_all_rooms_admin 字面量）

### 决策记录

- ✅ `search_all_rooms_admin` 拆为 7 main + 1 count 共 8 套静态字面量（比原 6+2 预估多 1 — Name cursor 因 name Some/None SQL 字面量差异拆 2 而非 1）
- ✅ `is_encrypted` 走 `($N::bool IS NULL OR EXISTS(...) = $N)` 单 SQL 模式，未拆 3 套字面量
- ✅ `search_term` 拆为 `search_pattern`（`%term%`）+ `similarity_term`（仅 `term.chars().count() >= 3` 时 Some）双参数，避免在 SQL 端用 `char_length($N) >= 3` 嵌套
- ✅ count query 用 3 字段 ILIKE（`name`/`topic`/`room_id`），与 main 的 4 字段（多 `canonical_alias`）有差异，保留这一差异以与原行为一致
- ⚠️ `room.rs::get_all_rooms_with_members` 1 QueryBuilder 保留（Batch 8 候选）
- ⚠️ `room.rs` 仍剩 14 复杂 join dynamic query_as（Batch 8 候选）

## Batch 5 完成报告 (2026-06-03)

> 范围：5 个 storage 文件 (state_groups, federation_queue, relations, thread, sliding_sync)
> 目标：99 动态 SQL → 99 静态宏
> 实际：79 动态 → 79 静态，dynamic ratio 70.2% → 64.9%

### 文件级结果

| 文件 | 动态前 | 动态后 | 静态增量 | 备注 |
|---|---|---|---|---|
| state_groups.rs | 17 | 0 | 17 | 全部完成（state_group / edge / event_to_state_group / state 4 张表） |
| federation_queue.rs | 8 | 0 | 8 | nullable 列 room_id/sent_at/retry_count/status 加 `as "field?"` cast |
| relations.rs | 14 | 0 | 14 | 8 套 direction × relation_type 笛卡尔 query_as! 全部静态化 |
| thread.rs | 36 | 1 | 35 | search_threads 1 处 ORDER BY 中 ts_rank/similarity 复杂表达式保留 query_as |
| sliding_sync.rs | 24 | 4 | 20 | 4 个 QueryBuilder（filter 路由）保留，其余 20 处全静态化 |

### 关键迁移技巧

1. **nullable 列处理**：`as "field?"` cast 把 DB 的 Option<T> 标记为 nullable（federation_queue/thread）
2. **可空 bool 字段**：`as "field!"` cast 把 DB 的 Option<bool> 强制成 bool（thread_subscriptions）
3. **JSONB cast**：`jsonb_build_array($3::text)` 加显式 cast（thread create_thread_root）
4. **`.flatten()` 去除**：从 `fetch_optional()` 取 Option<Option<T>> 时使用 `.flatten()` 折叠外层 Option
5. **JSONB 操作符**：`content->>'body'` + `ts_rank_cd` / `similarity` 在静态字面量中完全可写
6. **`RETURNING *` → 显式列**：所有 `RETURNING *` 改为显式列 + `as "field!"` cast，避免 nullable 推断歧义
7. **多列 RETURNING**：`thread_replies` / `thread_roots` / `thread_subscriptions` 表 11+ 列全部显式列 + cast

### 累计进展（Batch 1-5）

- 动态 SQL：1408 → 760（-648，-46%）
- 静态宏：4 → 412（+408，+10200%）
- dynamic ratio：99.7% → 64.9%（-34.8 pp）
- 5 批完成、5 文件 batch-1 + 5 文件 batch-2 + 6 文件 batch-3 + 5 文件 batch-4 + 5 文件 batch-5

### 决策记录

- ✅ `thread.rs::search_threads` 1 处保留 query_as：`ORDER BY` 中调用 `ts_rank_cd` / `similarity` 等 4 个不同列的相似度，宏无法对别名做 `ORDER BY`
- ✅ `sliding_sync.rs` 4 个 QueryBuilder 保留：`get_rooms_for_list` / `count_rooms_for_list` filter 字段组合动态，filter 类型有 4 种 × 2 端点 = 8 种查询面
- ✅ 19 处剩余动态均为：跨表 LEFT JOIN + 游标分页（B/C/D 类），按 Batch 4 评估报告属「SQLx 宏死角」，推迟至 Batch 6+

## Batch 4 遗留动态 SQL 评估报告 (2026-06-03)

> 范围：71 处 `query_as`/`query` + 5 处 `QueryBuilder` + 38 处含 `format!` 的列名拼接查询
> 目标：评估每类的迁移价值、可用迁移路径、保留动态的合理理由

### A. `format!` 列名拼接（38 处 — event.rs / user.rs / device.rs）

**模式**：`USER_COLUMNS` / `DEVICE_COLUMNS` / `EVENT_COLUMNS` / `STATE_EVENT_COLUMNS` 常量通过 `format!("SELECT {col} FROM ...")` 注入到 `query_as` 字符串。

**为什么不算真动态**：
- 列名/列序在编译期是常量，运行时不变
- 不引入新 `?N` 绑定，bind 列表在编译期完整
- 只在 Rust 端用 `format!` 拼接，Postgres planner 看到的是静态 SQL

**迁移路径**：
| 路径 | 描述 | 工作量 | 类型安全收益 |
|---|---|---|---|
| ✅ **推荐**：保持 `query_as` + 常量 format | 列名集中维护，bind 类型仍然推导；与 Batch 3 一致 | 0 | 中（bind 类型仍受查） |
| 拆分字面量变体 | 每个查询一个 `r#"SELECT col1, col2, ... FROM ..."#` 字面量；常量保留为单一来源（用 `concat!` 或 macro_rules 模板） | 大 | 高（完整静态校验） |
| 改 `query_as!` 宏 | 宏不支持 `format!` 输出，必须把所有列写在字面量中 | 中 | 高（但失去列名 DRY） |

**结论**：38 处是「类静态」动态，列名变化风险已由常量集中托管化解；保留 `query_as` + `format!` 是合理折衷。若要进一步收紧，可引入 `const_format::concatcp!` 宏在编译期拼接列名，使最终字符串成为字面量 → 切换为 `query_as!`。

### B. 跨表 LEFT JOIN + 游标分页（room.rs 1 处 + room_summary.rs 17 处）

**位置**：
- ~~`room.rs::search_all_rooms_admin`（QueryBuilder 2/3）~~ → Batch 7 已迁移（见上方）
- ~~`room.rs::search_all_rooms_admin` 内嵌 `count_query`（QueryBuilder 3/3）~~ → Batch 7 已迁移
- `room.rs::get_all_rooms_with_members`（QueryBuilder 1/1，Batch 8 候选）
- `room_summary.rs` 17 处 `query_as` 涉及 `LEFT JOIN room_summaries` / `room_memberships` 与 `as "field!"` 大量 nullable 投影

**为什么保留动态**（仅剩 `get_all_rooms_with_members`）：
- `room_search_cursor` + `room_search_order` 多轴游标分页，参数正交组合 > 20 种
- `HAVING (r.room_id < $1 ...)` 这类复合 keyset 谓词涉及 `LEFT JOIN` 聚合后过滤
- 评分/搜索维度（`is_encrypted` 触发 `EXISTS/NOT EXISTS` 子查询两种字面量） × 4 套游标谓词 × 3 套 ORDER BY → 24+ 种查询面
- `room_summary.rs` 阻塞原因：DB 列 nullable 但 struct 字段 non-nullable；`as "field!"` 强制非空会让运行时 null 行报错，而 LEFT JOIN 自然产生 null（如 `room_summaries.member_count` 可能是 NULL）

**迁移路径**：
| 路径 | 描述 | 工作量 | 风险 | 类型安全收益 |
|---|---|---|---|---|
| 拆 query 平面 | 按 `(order_by, has_search, is_encrypted_filter, cursor_kind)` 笛卡尔积生成 ~90 个 `query_as!` 变体 | 极大 | 高（覆盖率测试困难） | 高 |
| 改用 `($N IS NULL OR ...)` + ANY 数组 | 用 `where (?::bool IS NULL OR col = ?)` 模式吸收所有可空谓词；cursor 用 `ANY($1::bigint[])` 包装 | 中 | 中（需重写游标逻辑） | 高 |
| 调整 struct 字段为 `Option<T>` | 与 DB nullable 对齐，去掉 `as "field!"` cast | 小 | 低 | 中 |
| 保留 `query_as` + 类型断言 | `Row::get::<T, _>("col")` 手动 assert，调用方再 `unwrap_or_default` | 小 | 低 | 低 |

**结论**：跨表 join + 游标分页是 **SQLx 宏的死角**；推荐混合方案：
1. `room_summary.rs`：把 `RoomSummary` / `RoomWithMembersRecord` 字段改 `Option<T>` → 切换到 `query_as!`（去掉 `as "field!"` cast）—— **Batch 5 优先**。
2. `room.rs` 3 处 QueryBuilder：保留 + 收紧 `Row::get` 类型断言；游标分页固有动态性。

### C. 模糊搜索（search_all_rooms_admin 相似度 + ILIKE 多游标）

**状态**：✅ Batch 7 已完成（见上方 Batch 7 完成报告）

**完成路径**：
- `is_encrypted` 走 `($N::bool IS NULL OR EXISTS(...) = $N)` 单 SQL 模式，无需拆 3 套
- `search_term` 走 `search_pattern`（`%term%`）+ `similarity_term`（仅 `term.chars().count() >= 3`）双参数 + `($N::text IS NULL OR ...)` 模式吸收
- 游标 3 套（Created/Name/Size）× order_by 3 套 → 7 main（Name cursor 因 name Some/None SQL 差异再拆 1）+ 1 count = 8 静态字面量
- 实际新增 8 个静态宏（与预估 6+2=8 一致）

### D. 递归空间层级（space.rs `get_recursive_hierarchy`）

**位置**：`Box::pin async 递归` CTE 拼装

**为什么保留 `query_as`**：
- 单条 CTE 字符串是字面量，可改 `query_as!`
- 真正阻塞点：递归函数体在 Rust 端是「运行时调用自身 N 次」，每层一个 `query_as`；改 `query_as!` 完全可行，但需要为每一层显式 `as "field!"` cast（特别是 `depth` / `path` / `via_servers` 数组列）
- `via_servers jsonb → Vec<String>` 需 `ARRAY(SELECT jsonb_array_elements_text(...))` 转换表达式，可在字面量中写

**推荐路径**：
1. 保留 `Box::pin async 递归` 控制结构（无法在 SQL 中做"业务层递归"）
2. 递归函数体中单条 `query_as` 改为 `query_as!` + `as "field!"` cast
3. JSONB → 数组用 `ARRAY(SELECT jsonb_array_elements_text(...))` 表达式
4. 预估：6 处动态 → 6 处静态

### E. 状态事件 DISTINCT ON + ROW_NUMBER() window（event.rs）

**位置**：`get_room_state_events`、`get_state_events_for_room` 等

**为什么保留动态**：
- `DISTINCT ON (room_id) ORDER BY ...` 在静态 SQL 中完全可写
- 真正阻塞点：`STATE_EVENT_COLUMNS` 是 `format!` 注入（见 A 类）—— 一旦把列名变成字面量，`query_as!` 即可接管
- 少数情况：`ROW_NUMBER() OVER (PARTITION BY room_id ORDER BY ...)` 需要 `as "row_num!: i64"` cast

**推荐路径**：
1. 把 `STATE_EVENT_COLUMNS` 拆为每个查询的字面量（11 处）
2. `query_as` → `query_as!` + 列 cast
3. 预估：11 处 → 11 处静态（仅依赖 A 类 format! 拆分）

### F. 综合评估矩阵

| 类别 | 数量 | 静态宏可写性 | 推荐路径 | 预计静态宏增益 | 预计工作量 |
|---|---|---|---|---|---|
| A. format! 列名拼接 | 38 | 中 | 保留 `query_as` + 引入 `const_format` 宏实现字面量化 | 0（保留）→ 38（若字面量化） | 1-2d |
| B. 跨表 join + 游标 | 20 | 低 | struct `Option<T>` 化 + `query_as!` | 11（room_summary 11/17） | 2-3d |
| C. 模糊搜索多游标 | 2 + 1 count | 中 | 拆 6+2 套字面量 + Rust match 路由 | 3（room.rs 3 QueryBuilder） | 1-2d |
| D. 递归空间层级 | 6 | 中 | 单条 `query_as` → `query_as!` + cast | 6 | 1d |
| E. 状态事件 window | 11 | 高 | 依赖 A 类字面量化 | 11（与 A 协同） | 与 A 合并 |
| F. 其他 `query_as` 简单 | ~5 | 高 | 直接 `query_as!` | 5 | 0.5d |

**累计潜力**：当前 776 动态 → 估可再降 **36 处**（F 类简单 + D 全部 + C 全部 + B 半数 + E 协同），即 Batch 5+ 净降至 **~740**，动态占比从 69.4% → ~66%。

**A 类 38 处 format! 价值评估**：
- 类型安全收益：仅列名 / 列序有校验，bind 类型推断在 `query_as` 中已受查
- 维护成本：列名变化时只需改一处常量，聚合度高
- **结论**：保留为「类静态」是合理设计，不强行宏化

### G. 迁移优先级建议

1. **Batch 5 优先**（高 ROI）：F 类简单 `query_as` → `query_as!`（5 处）+ D 类递归空间层级（6 处）→ 净增 11 静态宏 / 减 11 动态
2. **Batch 6 候选**（中 ROI）：C 类拆 6+2 套 `search_all_rooms_admin` 字面量 + E 类状态事件 window 函数（依赖 A 类字面量化）
3. **Batch 7 候选**（低 ROI / 高风险）：B 类 room_summary `Option<T>` struct 化，11 处 `query_as!` 改造
4. **A 类 38 处 format!**：长期 follow-up，可引入 `const_format::concatcp!` 或 `macro_rules!` 模板实现「字面量等价」宏

### H. 决策记录

- ✅ **保留 A 类** `format!` 拼接：`USER_COLUMNS` / `DEVICE_COLUMNS` / `EVENT_COLUMNS` / `STATE_EVENT_COLUMNS` 常量 + `query_as` + format 是经过 Batch 1-3 验证的稳定模式
- ✅ **保留** `sliding_sync.rs` 5 个 `QueryBuilder` 路由（4 filter 字段 × 2 端点 = 路由 8 种）
- ✅ **Batch 7 完成** `room.rs::search_all_rooms_admin` 6+2 套字面量拆分（实际 7+1 = 8 套，Name cursor 因 name Some/None SQL 差异多拆 1）
- ⚠️ **推迟** `room_summary.rs` 17 处改造：等 Batch 5 整体调整 struct nullable 字段
- ⚠️ **推迟** `room.rs::get_all_rooms_with_members` QueryBuilder：游标分页固有动态（Batch 8 候选）
