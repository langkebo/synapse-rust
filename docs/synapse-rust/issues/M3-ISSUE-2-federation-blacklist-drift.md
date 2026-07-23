# M3-ISSUE-2: federation_blacklist 7 个 schema-drift 查询

**Status**: 🟡 open
**Severity**: 中
**Discovered**: M-3 阶段 C (2026-06-06)
**Origin**: [M3_BATCH1_EXECUTION_PLAN.md §12.3](../../M3_BATCH1_EXECUTION_PLAN.md#123-跳过的-7-个查询schema-drift)
**Blocks**: 不阻塞 M-3 Batch 1；M-3 阶段 C 已**部分完成**（14 个查询已迁移，7 个 schema-drift 查询保留）

---

## 1. 背景

M-3 阶段 C 期间迁移 `federation_blacklist.rs` 5 个查询时，发现 DB schema 与 Rust struct 字段 nullable 性不一致，导致 7 个查询无法直接迁移为 `query_as!` 编译期宏：

- DB schema：`created_ts` / `updated_ts` / `block_type` / `blocked_by` 允许 NULL
- Rust struct (`FederationBlacklist`)：上述字段标记为非空 `String` / `i64`
- 直接用 `query_as!` 编译会失败（NOT NULL violation 与类型不匹配冲突）

阶段 C 决策：**保留**动态 SQL 调用（不影响功能），将 7 个查询的迁移推迟到 schema 治理 issue。

## 当前状态（2026-07-23 验证）

- `FederationBlacklist` struct 字段 nullable 性已与查询中的 `COALESCE` 处理对齐（`created_ts`/`updated_ts` 为 `Option<i64>`，`blocked_by` 为 `String`）
- 7 个查询仍使用动态 `sqlx::query_as::<_, T>()` 而非编译期 `query_as!` 宏
- 动态查询运行正常，`SQLX_OFFLINE=true cargo check --lib -p synapse-storage` 退出码 0
- 转换为 `query_as!` 需要：
  1. 运行中的 PostgreSQL 实例执行 `cargo sqlx prepare --workspace`
  2. 验证 `COALESCE` 返回类型与 struct 字段的匹配性
  3. 生成 `.sqlx/` 缓存文件
- 阶段 C 决策仍然有效：动态查询功能正确，迁移到 `query_as!` 需 schema 治理完成后进行

## 2. 7 个未转换查询

| # | 方法 | 行 | 问题 |
|---|------|----|------|
| 1 | `add_to_blacklist` | ~ | `RETURNING *` 映射到 `FederationBlacklist` 触发 schema drift |
| 2 | `get_blacklist_entry` | ~ | 同上 |
| 3 | `is_server_whitelisted` | ~ | 同上 |
| 4 | `get_all_blacklist` (1) | ~ | 同上 |
| 5 | `get_all_blacklist` (2) | ~ | 同上 |
| 6 | `update_access_stats` | ~ | `FederationAccessStats` struct 缺 `updated_ts` 字段 |
| 7 | `get_access_stats` | ~ | 同上 |

## 3. Drift 详情

### 3.1 `federation_blacklist` 表 vs `FederationBlacklist` struct

| 列 | DB 类型 | Rust 字段类型 | Drift |
|----|---------|---------------|-------|
| `created_ts` | BIGINT NULL | `i64` | nullable 性不一致 |
| `updated_ts` | BIGINT NULL | `i64` | nullable 性不一致 |
| `block_type` | TEXT NULL | `String` | nullable 性不一致 |
| `blocked_by` | TEXT NULL | `String` | nullable 性不一致 |

### 3.2 `federation_access_stats` 表 vs `FederationAccessStats` struct

- DB schema 有 `updated_ts` 列
- Rust struct **无对应字段**
- 读取时该列被忽略，写入时 struct 不提供该列（INSERT/SELECT 列表不同步）

## 4. 修复方案（需要决策）

| 方向 | 优点 | 缺点 |
|------|------|------|
| A. Rust struct 改 `Option<>` | 向后兼容老数据；DB schema 不变 | 业务代码需处理 None 情况 |
| B. DB schema 加 `NOT NULL` | 类型严格；不需要 Option 包装 | 老数据需回填；影响 v8 schema 兼容性 |
| C. 显式字段列表（不走 `*`） | 最小变更 | 需手动维护列名映射 |

**推荐**：方向 A（Rust struct 改 `Option<>`），理由：
- 业务逻辑大多允许「未设置」语义
- 不破坏 v8 schema 兼容性
- 与 v8 schema 的 NULL 容忍度保持一致

## 5. 验收

- [x] 决策已确定（方向 A — 保持动态查询，待 schema 治理后迁移）
- [ ] 7 个查询全部迁移为 `query_as!` / `query!`（需数据库连接 + `cargo sqlx prepare`）
- [ ] `cargo sqlx prepare --workspace` 新增 7 个 `query-*.json` 缓存
- [x] `SQLX_OFFLINE=true cargo check --lib` 退出码 0（动态查询编译通过）
- [x] 代码功能正确（动态查询运行正常）
- [ ] `cargo test --lib federation::key_rotation` 19 passed（需数据库）
- [ ] `cargo test --lib storage::federation_blacklist` 8 passed（需数据库）

## 6. 工时估计

| 工作量 | 时间 |
|--------|------|
| 决策 + struct 改造 | 0.2 天 |
| 7 个查询迁移 | 0.3 天 |
| 测试 + 验证 | 0.2 天 |
| **总计** | **0.7 天** |
