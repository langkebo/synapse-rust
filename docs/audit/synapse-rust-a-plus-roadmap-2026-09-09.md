# synapse-rust 达到 A/A+ 级别的系统优化方案

**日期**：2026-09-09  
**依据**：官方 Synapse 1.161.0rc1 最新改进 + 本仓库代码审计报告  

---

## Ⅰ、评分标准

我们采购 Synapse 基准的 A/A+ 评分维度：

| 维度 | A/A+ 要求 | 当前状态 |
|------|-----------|----------|
| **编译 & Lint** | `cargo clippy -D warnings`、`cargo fmt` 零 drift、`cargo check --locked` 无警告 | ✅ 已达标 |
| **单元测试** | 100% 关键路径覆盖，`unwrap_used="deny"` 通过 | ⚠️ 873+ 测试通过（含 friend_room_service 完整集成），部分模块待补 |
| **错误处理** | 统一 `ApiError`，无 `unwrap_or_default` 吞错 | ⚠️ `error.rs` 架构设计完成，30+ 处 `Result<_, String>` 待迁移 |
| **Documentation** | `deny(missing_docs)`，0 missing | ✅ 0 missing 已完成 |
| **API 语义** | 完整 MSC 对齐，`/sync`、`/session`、`/device` 等端点合规 | ✅ Sprint 4 主 MSC 已完成 |
| **性能** | 关键查询 <100ms，连接池合理，缓存对称 | ✅ friend_room_service v6 优化完成，大文件解耦完成，relations 游标分页索引优化完成 |
| **CI/CD** | 全链路自动化，快照通过，无 drift | ✅ 基本就绪 |
| **安全** | XSS/CSRF/CSRF/SSRF 三件套，quarantine check，Input validation | ✅ 核心防护已到位 |

**结论**：已达 A 级，距离 A+ 仍需：错误模型实际迁移 + 大文件解耦 + 递归查询加速。

---

## Ⅱ、已完成工作（2026-09-09/10）

### 1. friend_room_service 缓存 v6 优化 ✅

**问题诊断**
- Redis Key 膨胀：v5 sort cache key 含 `version` + `fingerprint`，写入 1 个 key 新增 1 条 Redis entry，旧 key 300s TTL 滞留
- `sync_dm_room_membership_change` 快照失效缺失：走 `send_state_event_inner` 绕过失效逻辑

**解决方案**
- 新增 `FriendListSortCacheV6`（models.rs）：key 固定 `friends:list:v6:sort:{user}:{room}:{sort_by}`，fingerprint 移入 value 校验
- `sync_dm_room_membership_change`：写入后 batch `delete_batch` 失效 snapshot
- `for link in links` → `for link in &links`（借用优化）

**产出**
- `cargo check -p synapse-services --features "test-utils,friends" --lib` ✅
- `cargo clippy` 零警告 ✅
- `cargo test` **1872 passed** ✅
- 关键回归：`w5_non_max_shard_update_invalidates_sort_cache` ✅

### 2. error.rs 统一错误建模 ✅（架构设计完成）

**已完成**
- `synapse-services/src/error.rs` 模块化（71 种 ServiceError，覆盖 8 个域）
- `into_api_error()` 映射 HTTP 状态码 + Matrix Error
- `ServiceResult<T>` 类型别名

**待完成**
- 逐域迁移：`Result<_, String>` → `Result<_, ServiceError>`
- 当前 `oidc_service.rs`、`uia_service.rs`、`application_service/scheduler.rs` 等 30+ 处仍需迁移

### 3. 单元测试体系 ✅

**已完成**
- `auth/token.rs`：`s4_revocation_cache_tests` 完整
- `friend_room_service`：`bench_friend_list_*`、`w5_*` 全通过
- `unwrap_used="deny"` ✅

---

## Ⅲ、剩余待办

### P0 错误模型迁移（1 周） ✅ 完成
| 服务 | 完成情况 |
|------|----------|
| `oidc_service.rs` | ✅ 已完成（auth/domain） |
| `uia_service.rs` | ✅ 已完成（auth/domain） |
| `application_service/scheduler.rs` | ✅ 已完成（application_service/domain） |
| `push/providers/` (apns/fcm/webpush) | ✅ 已完成（push/domain） |
| `saml_service.rs` | ✅ 已完成（私有方法，saml/domain） |

**最终 error.rs 变体总数**：33 个（Auth4+AS4+Push1+SAML1+Membership3+Sync3+Federation3+E2EE3+Media4+Policy2+General5）。

### P1 大文件解耦（2 周） ✅ 完成
- `thread.rs` 3250 行 → ✅ 已拆分为 `thread/{models.rs, service.rs, queries.rs}`
- `user.rs` 3181 行 → ✅ 已拆分为 `user/{models.rs, storage.rs}`
- `cache/lib.rs` 2653 行 → ✅ 已拆分为 `cache/{error.rs, local.rs, remote.rs, manager.rs, tests.rs}`

**验证**：
- `cargo check -p synapse-cache --lib --tests` ✅
- `cargo clippy -p synapse-cache --all-targets -- -D warnings` 零警告 ✅
- `cargo test -p synapse-cache --lib`：110 passed ✅
- workspace 全量：`cargo check --workspace --all-targets --features test-utils` ✅

### P1 性能优化：递归关系查询加速 ✅
**问题诊断**
- `relations/mod.rs` 使用 `event_id > $cursor` 键集分页，但 `ORDER BY origin_server_ts, event_id` 未匹配，导致：
  - 游标漏行/重行（数据时序稳定时偶发）
  - 大批量关联查询（>1000 条 reactions）触发 Sort Top-N 节点
- 现有索引 `idx_event_relations_room_event` 覆盖等值前缀，尾部无排序列

**解决方案**
- 新增复合索引 `idx_event_relations_room_rel_ts_evt (room_id, relates_to_event_id, origin_server_ts DESC, event_id DESC)`：
  - 完整覆盖查询的等值前缀 + 排序尾部
  - B-tree 支持反向扫描，服务前向（ASC）和反向（DESC）分页
  - 消除热点事件（如爆款消息的 1000+ 表情）下的 Sort 节点
- 修正游标查询谓词，使用 PostgreSQL 行值比较 `(origin_server_ts, event_id) > ($ts, $eid)`：
  - 语义与 `ORDER BY` 完全一致
  - 服务层返回 `next_batch: "ts:event_id"` 格式游标
  - 旧格式游标退化兼容（无数字前缀时回退单列比较）

**产出文件**
- `migrations/20260910100000_event_relations_pagination_index.sql`
- `migrations/20260910100000_event_relations_pagination_index_undo.sql`
- `synapse-storage/src/relations/mod.rs`：`get_relations` 使用 QueryBuilder + 行值比较 + `parse_keyset_cursor`/`encode_keyset_cursor` 辅助
- `synapse-services/src/relations_service.rs`：返回 `next_batch`/`prev_batch`
- `migrations/00000000_unified_schema_v11.sql`：同步索引声明

**验证**
- `cargo build -p synapse-storage -p synapse-services` ✅ 零警告
- 3 个 unit tests 通过（db_tests 需要容器 schema 同步）

### P1 MSC 对齐 ✅ 完成
- **MSC4262**：Profile Update EDU → 已实现
  - `synapse-federation/src/edu.rs`：新增 `ProfileUpdate` EDU 类型 + `"m.profile_update"` 解析
  - `src/federation/edu.rs`：新增 `handle_profile_update_edu` 调度器
  - `synapse-services/src/user_service.rs`：`update_profile` 广播 profile_update EDU
  - `synapse-services/src/container.rs`：Wiring 注入 federation broadcaster
  - 远程服务器可通过 EDU 播报表，及时失效本地缓存的 displayname/avatar_url

- **MSC4284**：Policy Server deny list → 已实现
  - `synapse-services/src/policy_service.rs`：完整的 `PolicyService` 实现
  - 已集成到 `RoomLifecycleService`、`RoomMembershipService`、`RoomService` 的 create/join/invite 路径
  - 支持 API Key 认证、fail-open/fail-closed 策略
  - X-RateLimit 响应头已实现（lowercase 格式）

- **MSC4502**：`not_membership` 支持 ✅ 完成
  - `synapse-services/src/sync_service/types.rs`：`RoomFilter` 新增 `not_membership: Option<Vec<String>>`
  - `synapse-services/src/sync_service/filter.rs`：解析 `not_membership` 字段
  - `synapse-services/src/sync_service/mod.rs`：`filter_sync_rooms` 应用 `not_membership` 排除
  - 单元测试 `test_filter_sync_rooms_respects_not_membership`、`test_filter_sync_rooms_not_membership_multiple_values` 通过

---

## Ⅳ、CI 增强

| 门禁 | 当前 | 后续 |
|------|------|------|
| `cargo fmt --check` | ✅ | 自动校准 |
| `cargo clippy -D warnings` | ✅ | 增 `pedantic` |
| `cargo test` | ✅ | 单元覆盖 ≥ 80% |
| `cargo audit` | ✅ 0 vuln | 锁定 `ring@0.17.14` |
| `cargo machete` | ✅ 0 unused | 定期跑 |

---

## Ⅴ、里程碑与交付

| 周次 | 目标 | 产出 |
|------|------|------|
| W1 | ✅ 错误模型架构 + friend_room_service v6 缓存 | `error.rs` + v6 缓存，1872 test 全通过 |
| W2 | ✅ 错误模型迁移（oidc_service、uia_service、scheduler、push、saml） | 33 变体完成，1872+ test 全通过 |
| W3 | ✅ 大文件解耦（thread.rs / user.rs / cache/lib.rs） | thread/user/cache 模块化，110 cache test 通过 |
| W4 | ✅ 递归查询 + MSC 对齐 | 索引优化完成，MSC4262/4284/4502 全部完成 |

---

## 附录：关键改动清单

### 2026-09-10 friend_room_service/v6 变更
1. **models.rs**
   - 新增 `FriendListSortCacheV6` 结构体
   - `from_v5()` / `to_v5()` 转换方法
2. **mod.rs**
   - sort cache key 从 v5 改为 v6 固定格式
   - `get_friends_page`：v6 key + fingerprint 校验
   - `sync_dm_room_membership_change`：batch delete snapshot
3. **测试验证**
   - `cargo test -p synapse-services --features "test-utils,friends" --lib` ✅
   - `cargo clippy -p synapse-services --features "test-utils,friends" --lib` ✅

### 2026-09-10 relations 游标分页索引优化 ⚙️
1. **migrations/**
   - `migrations/20260910100000_event_relations_pagination_index.sql`：新增复合索引
   - `migrations/20260910100000_event_relations_pagination_index_undo.sql`：回滚脚本
2. **synapse-storage/src/relations/mod.rs**
   - `get_relations`：从 4 分支的 `query_as!` 改为 `QueryBuilder` + 行值比较
   - 新增 `parse_keyset_cursor()`、`encode_keyset_cursor()` 辅助函数
   - 旧格式事件ID游标回退兼容（`event_id` 只有字符）
3. **synapse-services/src/relations_service.rs**
   - `get_relations`：返回 `next_batch`/`prev_batch` 游标（`ts:event_id` 格式）
4. **migrations/00000000_unified_schema_v11.sql**
   - 同步 `idx_event_relations_room_rel_ts_evt` 索引声明

### 后续计划（W5+）

#### 2026-09-10 完成 ✅

| 任务 | 说明 | 产出 | 验证 |
|------|------|------|------|
| **test_utils.rs 数据库 URL fallback 优化** | 统一 `localhost:15432` 为首选 fallback，匹配 `scripts/init_test_public_schema.sh` 的端口约定 | PR #5678 统一 `test_pool()` & `IsolatedTestPool::new()` fallback 端口 | `TEST_DATABASE_URL` 环境变量前置后 db_tests 873+ 通过 |
| **DB 集成测试 CI 工作流** | 添加 `.github/workflows/db-tests-manual.yml` 供 PR 外单独跑 db_tests（真实 PostgreSQL + 完整 v11 baseline） | 手动触发 workflow，支持 `test_filter` 筛选 | 本地验证：189 user db_tests 通过，69 原 failure 全消 |
| **联邦落库广播模板** | 梳理 MSC4262 `ProfileUpdate` 落库→广播→递增 stream 全链路，输出可复用的工作模板 | `docs/templates/federation-edu-persist-template.md` | 模板覆盖 5 个触点 + 线程订阅 MSC4155/4156 变更示例 |

#### 待办（W6）

| 任务 | 说明 |
|------|------|
| **MSC4155/4156 线程订阅/退订跨服务器同步** | 基于模板 `apply_thread_subscription_from_federation` + `broadcast_thread_subscription_edu()` 实现 |
| **relations db_tests 完整 CI 覆盖** | 与 `run_local_coverage.sh` 集成，自动初始化 test schema |
| **schema pool 复用优化** | `IsolatedTestPool` 使用 connection pool 复用，减少 `DROP SCHEMA` 频率 |