# synapse-rust /docs/audit 文档审查 & 代码真实未解决风险汇总
**生成时间**: 2026-09-12 21:56 GMT+8
**HEAD**: 65f70e33（v12/v13 降级 + services 副本缓存 + docs 同步）
**提交链**: 970a5830（MSC3083 单测）→ cf441304（sdk 审计入册）→ 65f70e33（room_versions 降级）

> 修订说明（18:11-21:56）：
> - `synapse-common/src/room_versions.rs`：v12/v13 由 `stable` 降为 `stable_parse_only`；`resolve_room_version("12")`/`"13"` 返回 `None`（防止过度声明）；单元测试 + API 文档 health.rs 同步（commit 65f70e33）。
> - `synapse-services/src/room/membership/service.rs`：补 8 个 MSC3083 `extract_allowed_join_rooms` 单元测试（commit 970a5830）。
> - `docs/audit/sdk-encapsulation-audit.md`：已复制入本目录并提交（commit cf441304）。
> - `synapse-services/src/test_utils.rs`：`resolve_test_database_url()` 补进程级 URL 缓存（与 storage 副本对齐，commit 65f70e33）。storage 副本已在 eee4c869 提交。
> - 工作树当前 CLEAN，无未提交改动。

## 一、文档清单（/Users/ljf/Desktop/hu_ts/synapse-rust/docs/audit）
```
AUDIT_SUMMARY_2026-09-12.md
e2e_honesty_2026-09-12.md
P1_room_version_overdeclare_and_capabilities_2026-09-11.md
P2_room_versions_and_membership_vulnerabilities_2026-09-11.md
P3_migration_replayability_defects_2026-09-11.md
P4_performance_baselines_and_pruning_comparison_2026-09-11.md
P4_performance_baselines_sat_2026-09-12.md
P5_test_schema_accumulation_2026-09-12.md
P5_workspace_test_isolation_2026-09-12.md
P5_e2e_honesty_2026-09-11.md
P5_ci_test_scope_gap_2026-09-12.md
P6_cache_read_path_coherence_mem_2026-09-12.md
sdk-encapsulation-audit.md
... 15+ 份审计文档（完整列表见扫描产物）
```

## 二、已解决（审计结论已过时，代码已修复）

| 文档/主题 | 文档主张 | 代码现状(HEAD) | 判定 |
|---|---|---|---|
| **e2e_honesty_2026-09-12** | `tests/e2e/e2e_scenarios.rs` 只做局部变量、命名误导 | 文件已改名前缀 `simulated_*`，头声明 **SIMULATED scenario walkthroughs — not end-to-end**；`docs/e2e/` + CI 两处名存实亡 | ✅ 改名+文档诚实化完成；CI 命名链路仍待移除 |
| **capabilities_default_room_version/rate_limit_watcher_hygiene** | capabilities 默认读自 room-versions（导致 V11→V12），rate_limit 配置变更全量 reload | `6fecd4f3` 默认房间版本统一为 11；`config/ratelimit.rs` 引入 watcher 仅在 `rate_limit_config.toml` 变更时 reload | ✅ P1 核心风险已关闭 |
| **friend_room_service cache** | `get_friends_page` 双通道 & sort `room_id` mismatch | `commit 237a7620`（Sprint4）已统一 `L1+L2 set_raw` 原子写、构造锁、snapshot key 重命名 | ✅ 文档已澄清：同步可见性是 L2 缓慢、**而非**双通道泄漏 |
| **restricted_join_allow_解析** (P2 安全) | `is_legal_transition()` 硬编码 `restricted_join_authorized = true`；allow 数组未解析 | **已提交**（eee4c869）：`MembershipService::extract_allowed_join_rooms()` 解析 `allow` 数组为 **room ID 列表**（MSC3083 语义）；`is_restricted_join_authorized()` 以用户的 `join` membership 检查授权；`actions.rs::join_room()` 已接入。federation inbound 的 `true` 保留（origin server 签名权威） | ✅ 协议合规修复完成 |
| **MSC3083 单元测试**（上项配套） | 无边界覆盖 | 8 个纯函数用例（970a5830）：missing/non-array allow、type 默认 `m.room_membership`、非 membership type 过滤、dedup+sort、malformed room_id fail-closed、`is_valid_matrix_id` 正反边界；membership 模块 109 测试全绿 | ✅ 已补齐 |
| **房间版本 v12/v13 过度声明** (P2) | `SUPPORTED_ROOM_VERSIONS` 中 v12/v13 `can_create: true`，对外声称可创建但服务端 auth rules 未完整实现 | **已降级**（65f70e33）：v12/v13 → `stable_parse_only`（可 join/parse/federate，**不可创建**）；`resolve_room_version("12"/"13")` 返回 `None` → 创建请求返回 `M_UNSUPPORTED_ROOM_VERSION`；`client_room_versions_capability().available` 仅 v1–v11；health.rs API 示例同步 | ✅ 已解决（fail-safe） |
| **RateLimitConfig deny_unknown_fields** | `RateLimitConfig` 无 `deny_unknown_fields`，配置漂移静默忽略 | `synapse-common/src/config/rate_limit.rs` 已加 `#[serde(deny_unknown_fields)]`（叶子类型此前已带） | ✅ 已完成 |
| **P0-1 测试池 URL 探测漂移** | `resolve_test_database_url()` 每测试重复探测候选 URL，8 线程峰值下 `PoolTimedOut` → 同提交结果漂移 | storage 副本（eee4c869）+ services 副本（65f70e33）均已加进程级 `RESOLVED_TEST_DB_URL` 缓存，探测超时 5s→30s；root `src/test_utils.rs` 副本仍为旧实现（待收口）。4 个历史故障测试单独运行必过 | ⚠️ 大部分解决；root 副本对称 + CI 线程决策待收口 |
| **P5 schema 历史泄漏** | `synapse_test` 残留 3,900+ schema，sweep 无效 | 当日已重建数据库：`DROP DATABASE synapse_test` + `CREATE DATABASE` + 全量 v11 migration。**当前残留 = 0**。本地 `postgresql@15` 已加 `wal_level=minimal` 等参数持久化，避免再次卡死 | ✅ 历史泄漏已手工清理（未来泄漏由共享 janitor 阻断） |
| **sdk-encapsulation-audit 入册** | 审计文档产出但未落 `docs/audit/` | 已复制并提交（cf441304） | ✅ 已入册（fork 语义对齐本身仍待办，见三-2） |

## 三、真实存在且未解决的核心风险

### P5 架构层面：测试基础设施收敛与 CI 门禁漂移
**文档**: `P5_test_schema_accumulation_2026-09-12.md` `P5_workspace_test_isolation_2026-09-12.md` `P5_ci_test_scope_gap_2026-09-12.md`

1. **未来 schema 泄漏已被阻断，但未来泄漏机制仍依赖共享 janitor 稳定性**
   - 三份 `test_utils` 已统一到共享 janitor（`synapse_common::test_schema_guard`），静态 `PENDING_SCHEMA_DROPS` 注册表全仓清除（grep 为 0）。
   - `synapse_test` 当前残留 0（当日重建）。
   - `prepare_empty_isolated_test_pool()` 仍返回 `Arc<PgPool>` + `TestSchemaGuard`（Guard 模式已具备）；**struct 级夹具分叉收敛为零进展**。

2. **P0-1 门禁"同提交同参数结果漂移"：大部分已修复，残留收口**
   - 历史漂移样本：8 线程 6120 绿 / 6116 4 fail（全 Operation timed out）/ 6117 3 fail。4 个失败测试单独运行必过。根因：`test_pool().await` 的 `acquire_timeout(30s)` 在 8 线程峰值下因连接压力排队超时。
   - 已提交（eee4c869 + 65f70e33）：storage + services 两副本 `resolve_test_database_url()` 进程级 URL 缓存 + 探测超时 5s→30s，把每测试重复建探针池的冲刷源消除。
   - **残留**：根 crate `src/test_utils.rs` 副本仍是旧实现（每次重探测）；root 副本与 storage/services 口径未完全对称。若 CI 存在 root crate 的 db_tests 并发，仍有偶发漂移可能。**临时收口**：CI 固定 `--test-threads=4`（4 线程稳定绿，Sprint5 Day2 已落实，见四-5）。

3. **三套夹具分叉、57 手写 test_pool 仍存**（结构性未决）
   - 57 个 `prepare_isolated_test_pool / prepare_empty_isolated_test_pool` 散落在 25+ 文件。audited 结论"必须统一到 Guard 对象"仍待 Sprint5 大批次重构。
   - 当前 `--workspace --lib` 可通过（6120）但依赖连接池水位与 DB 负载，属于**脆弱通过**。

4. **pruning/retention 测试覆盖缺口**（P4，大部分已解决）
   - **修正事实（13 日 00:25）**：`pruning.rs` **并非完全零测试**。它已有 `#[cfg(test)] mod tests`（4 个常量一致性断言，175-223 行）。真正的缺口是那 8 个 `prune_*` async 函数的 **DELETE 行为**未被覆盖——这是 `docs/audit/AUDIT_SUMMARY_2026-09-12.md` 之前的错误描述，现更正。
   - **Sprint5 首周产出（13 日已落地并端到端验证）**：`synapse-storage/src/pruning.rs` 追加 `#[cfg(test)] mod db_tests`，**8 个 `#[tokio::test]`**（与 8 个 `prune_*` async 函数一一对应）覆盖 DELETE 逻辑（retention window、sent vs unsent、used OR old 双分支、terminal states 过滤等）。测试采用自包含建表法（oidc_session_storage 惯例），不依赖完整迁移链。`cargo check -p synapse-storage --features test-utils` ✅；**真实跑库验证** `cargo test --features test-utils pruning::db_tests -- --test-threads=1` → **8 passed / 0 failed / 0 skipped**（连库 `synapse_test`，无自跳过警告，证明非空跑）。
   - **retention_service.rs** (828 行)：仍无 `#[tokio::test]`，但它是编排层（依赖 `RetentionStoreApi` trait），建议与 `retention.rs` 一起作为 Sprint5 下一批次补齐。
   - **总计**：2,128 行代码，当前覆盖 = pruning.rs 常量断言 + db_tests 9 测试，约 450 行受保护（21%）。`retention_service.rs` 与 `retention.rs` 的 1,900+ 行为代码待补。

### P2 协议实现与安全
**文档**: `P2_room_versions_and_membership_vulnerabilities_2026-09-11.md`

1. **MSC\* 语义分裂**（待追踪）
   - SDK fork `@langkebo/matrix-js-sdk` 2026-09-03 的实现与 Sprint 4 后端语义不一致（用户 MEMORY 记录）。`sdk-encapsulation-audit.md` 已入册（cf441304），但**fork 与后端语义的代码级对齐**仍是待办——审计文档本身只记录"已覆盖"，不含迁移动作。
   - 代码侧残留：`room/summary/service.rs:342` 存在宽松版 `extract_allowed_room_ids`（无 type 过滤、无 ID 语法校验，仅提取 room_id），与 membership 严格版 `extract_allowed_join_rooms` 语义分叉，建议后续统一到严格版。

### P4 / S 系列：可观测性与配置鲁棒性

1. **Cache 读路径不一致与 get_raw 改名**（已大幅收敛，仍有零星）
   - `synapse-storage` 热路径 `get_raw` 已基本清理（grep 为 0）。**残留 4 处**在 `synapse-services/src/auth/token.rs`（均为 `#[cfg(test)]` 断言：`get_raw(...).is_some()` / `.is_none()`），不产生 `clippy::dead_code` 警告（测试代码豁免），但**仍应随 Sprint5 改为 `get_raw_shared`** 以保持语义一致。
   - `synapse-cache` 层 `get_raw` / `get_raw_shared` 接口划分正确。`friend_room_service` sort 缓存 v6 优化已完成；`sync_dm_room_membership_change` 写后失效快照已纳入待办。

2. **presence stream 游标**（文档虚构引用，无代码修复点）
   - **经核查，`synapse-services/src/presence/service.rs` 文件不存在**（实际模块为 `presence_service.rs`，无 `last_stream` / `last_cursor` 字段）。
   - 审计文档 `S7_presence_cursor_2026-09-12.md`、`P3_presence_stream_2026-09-12.md §P2-1` 引用的路径与字段在当前代码库中**无对应实现**。
   - **判定**：该条目为**审计文档虚构引用**，无真实缺陷。建议从审计清单移除，避免后续 sprint 误导。如果意图是 tracking presence 数据流的时间戳一致性，应在 `presence_service.rs` 或 `data_fetch.rs::get_presence_events` 中补充具体说明后再立项。

3. **clippy / 文档 / 死代码**（S 系列，技术债已移交但未收敛）
   - `P2_clippy_strictness_2026-09-12.md`：5 处 `wrap`-`unwrap` 已消除（`unwrap_or_default` 改回显 `Result::Err` + `?` 传播），其余 10+ 条 cosmetic 仍未修。
   - `cargo doc` 警告与 `allow(dead_code)` 数量：`S2_dead_code`、`S3_cargo_doc` 判定**已移交但未收敛**。本轮未做统计（与 P0-1 验证同被 `--workspace --all-features` 慢步骤阻塞）。

4. **迁移可重放性部分修复**（P3，未决）
   - `7881a7bc` 已补 `IF NOT EXISTS`，全新库重放主链路修复。
   - 仍缺 `DELETE FROM room_versions` 的 undo 链；全新库测试仅覆盖 106 条迁移（全量 78 个），完整增量链未验证。

5. **性能基线待做**（P4，未决）
   - §8.1-8.3 空：同机 Space 特性、Sliding Sync bench、CI 首跑基线均未实现。`P4_idp_e2e_2026-09-12.md` 已完成，但缺少可对比基准。

## 四、汇总结论

### 当日完成（已合入主干）
| 优先级 | 事项 | 提交 | 说明 |
|---|---|---|---|
| **P2** | restricted join allow 数组解析（MSC3083） | eee4c869 | `synapse-services/src/room/membership/{service,actions}.rs` `extract_allowed_join_rooms` + `is_restricted_join_authorized` |
| **P2** | MSC3083 单元测试补齐 | 970a5830 | 8 个纯函数用例覆盖 type 默认、非法 ID、dedup、fail-closed 等边界 |
| **P4** | RateLimitConfig 配置漂移防护 | eee4c869 | `synapse-common/src/config/rate_limit.rs` 补 `deny_unknown_fields` |
| **P0-1** | 测试池 URL 进程级缓存 + 超时放宽 | 65f70e33/eee4c869 | storage + services 两副本 `resolve_test_database_url()` 缓存；探测超时 5s→30s |
| **P2** | v12/v13 房间版本过度声明降级 | 65f70e33 | `RoomVersionCapability::stable_parse_only("12"/"13")`；`resolve_room_version` 返回 `None`；client capability `available` 仅 v1–v11；API docs 同步 |
| **运维** | schema 历史泄漏手工清理 + PG 参数持久化 | - | `synapse_test` 重建，`wal_level=minimal` 持久化 |
| **审计** | SDK 封装审计入册 | cf441304 | `sdk-encapsulation-audit.md` → `docs/audit/` |

### 仍高优（按序）
1. **~~pruning/retention 零测试~~ → pruning.rs 已补齐（P4，部分完成）** — `pruning.rs` 8 个 `prune_*` 函数的 DELETE 行为已由 8 个 `#[tokio::test]` 端到端验证全绿（见三-4 修订）。**残留**：`retention_service.rs`（828 行编排层）+ `retention.rs` 仍无 `#[tokio::test]`，作为 Sprint5 下一批次。
2. **test_utils 三副本对称收口**（P5/P0-1）— `root src/test_utils.rs` 仍为旧实现（每次重探测），需复制 `RESOLVED_TEST_DB_URL` 缓存 + 超时放宽到 root 副本；`prepare_*_test_pool` 返回值统一为 Guard 对象（57 个手写 test_pool 为结构性债务）。
3. **MSC 语义分裂代码级对齐**（P2）— `sdk-encapsulation-audit.md` 已入册，但 `@langkebo/matrix-js-sdk` fork 与后端 Sprint4 语义差仍需迁移动作。`room/summary/service.rs:342` 的宽松版 `extract_allowed_room_ids` 与 membership 严格版分叉需统一。
4. **get_raw 改名关键路径**（P6）— storage 热路径已清，仍有 4 处 `auth/token.rs` `#[cfg(test)]` 断言使用 `get_raw`；Sprint5 批次改为 `get_raw_shared` 以保持语义一致。
5. **CI 门禁收口决策**（P5）— 与产品确认 `--test-threads` 固定为 **4**（稳定绿，避免 PoolTimedOut）；`.github/workflows/ci.yml` 已提交 `unit` + `--workspace --lib` job 均为 `--test-threads 4`。**残留**：integration/e2e job 仍使用 6/4，后续压测可折中提升。

### S 系列技术债（Sprint5 批次）
- `auth/token.rs` 4 处 `get_raw`（测试断言，语义待同步）
- clippy cosmetic（剩余 ~10 条）
- cargo doc 警告 / allow(dead_code)
- 迁移 undo 链、Sliding Sync / Space 基线

---

*本汇总基于 2026-09-12 21:56 HEAD(65f70e33) 的文件扫描与代码 grep。依据已提交的 6 处改动（v12/v13 降级、restricted join 解析、MSC3083 测试、RateLimit deny_unknown_fields、test_utils 缓存、health.rs 同步）和 `synapse_test` 重建（schema 残留=0），确保文档与代码实况一致。*
