# synapse-rust /docs/audit 文档审查 & 代码真实未解决风险汇总
**生成时间**: 2026-09-12 21:32 GMT+8
**HEAD**: cf441304（audit 提交）→ 970a5830（MSC3083 单测）→ eee4c869（v12/v13 修复） 当前工作区

> 修订说明（18:11-22:45）：
> - `synapse-common/src/room_versions.rs`：v12/v13 由 `stable` 降为 `stable_parse_only`；`resolve_room_version("12")`/`"13"` 返回 `None`（防止过度声明）；更新单元测试以匹配新语义；同步 API 文档 health.rs client capability 示例。
> - `synapse-services/src/room/membership/service.rs`：补 8 个 MSC3083 `extract_allowed_join_rooms` 单元测试，覆盖边界（非成员类型、malformed ID、dedup+sort、default 类型）。
> - `docs/audit/sdk-encapsulation-audit.md`：已复制入本目录，完整记录 SDK fork 包装覆盖情况。
> - CI workflow：注释已确认 `--test-threads 8` 已调高；P0-1 漂移根因 `test_utils` URL 缓存在 storage 副本提交，但 services 副本尚未提交。

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
| **restricted_join_allow_解析** (P2 安全) | `is_legal_transition()` 硬编码 `restricted_join_authorized = true`；allow 数组未解析 | **代码已实现**（工作区待提交）：`MembershipService::extract_allowed_join_rooms()` 解析 `allow` 数组为 **room ID 列表**（MSC3083 语义）；`is_restricted_join_authorized()` 以用户的 `join` membership 检查授权；`actions.rs::join_room()` 已接入。federation inbound 的 `true` 保留（origin server 签名权威） | ✅ 协议合规修复完成；需追加单元测试并合入主干 |
| **RateLimitConfig deny_unknown_fields** | `RateLimitConfig` 无 `deny_unknown_fields`，配置漂移静默忽略 | `synapse-common/src/config/rate_limit.rs` 已加 `#[serde(deny_unknown_fields)]`（叶子类型此前已带） | ✅ 已完成 |
| **P5 schema 历史泄漏** | `synapse_test` 残留 3,900+ schema，sweep 无效 | 当日已重建数据库：`DROP DATABASE synapse_test` + `CREATE DATABASE` + 全量 v11 migration。**当前残留 = 0**。本地 `postgresql@15` 已加 `wal_level=minimal` 等参数持久化，避免再次卡死 | ✅ 历史泄漏已手工清理（未来泄漏由共享 janitor 阻断） |

## 三、真实存在且未解决的核心风险

### P5 架构层面：测试基础设施收敛与 CI 门禁漂移
**文档**: `P5_test_schema_accumulation_2026-09-12.md` `P5_workspace_test_isolation_2026-09-12.md` `P5_ci_test_scope_gap_2026-09-12.md`

1. **未来 schema 泄漏已被阻断，但未来泄漏机制仍依赖共享 janitor 稳定性**
   - 三份 `test_utils` 已统一到共享 janitor（`synapse_common::test_schema_guard`），静态 `PENDING_SCHEMA_DROPS` 注册表全仓清除（grep 为 0）。
   - `synapse_test` 当前残留 0（当日重建）。
   - `prepare_empty_isolated_test_pool()` 仍返回 `Arc<PgPool>` + `TestSchemaGuard`（Guard 模式已具备）；**struct 级夹具分叉收敛为零进展**。
   - 当日修复了漂移根因：`resolve_test_database_url()` 加了进程级缓存（`RESOLVED_TEST_DB_URL`）+ 探测超时 5s→30s。效果：`--workspace --lib` 稳定在 6120 通过；但**未提交的修复仍在工作区**，需合入主干后生效。

2. **P0-1 门禁"同提交同参数结果漂移"仍是公开风险**
   - 历史漂移样本：8 线程 6120 绿 / 6116 4 fail（全 Operation timed out）/ 6117 3 fail。4 个失败测试（`voice::db_tests::test_round_trip_all_fields`、`user::db_tests::test_update_password`、`widget::db_tests::create_and_get_widget`、`widget::db_tests::delete_widget_permission_hard_deletes`）单独运行必过。根因已定位：`test_pool().await` 的 `acquire_timeout(30s)` 在 8 线程峰值下因连接压力排队超时。
   - 当日修复方向正确，但**未提交即未生效**。建议 CI 固定 `--test-threads=4`（4 线程下稳定绿）作为临时收口。

3. **三套夹具分叉、57 手写 test_pool 仍存**（结构性未决）
   - 57 个 `prepare_isolated_test_pool / prepare_empty_isolated_test_pool` 散落在 25+ 文件。audited 结论"必须统一到 Guard 对象"仍待 Sprint5 大批次重构。
   - 当前 `--workspace --lib` 可通过（6120）但依赖连接池水位与 DB 负载，属于**脆弱通过**。

4. **pruning/retention 零测试覆盖**（P4 真实未决）
   - 代码真实规模：**2,128 行**（`retention_service.rs` 828 + `pruning.rs` 223 + `retention.rs` 1,077），**均无任何 `#[tokio::test]` / `#[test]` / `cfg(test)` 模块**（grep 为 0）。
   - `tests/integration/` 与 `tests/` 下无任何 `pruning\|retention\|auto_delete` 用例。
   - 这是 P4 §8 的"对比基准缺失"与"零覆盖"的双重问题，规模不小的代码处于完全无测试保护状态。

### P2 协议实现与安全
**文档**: `P2_room_versions_and_membership_vulnerabilities_2026-09-11.md`

1. **房间版本 v12/v13 过度声明**（未决）
   - `synapse-common/src/room_versions.rs` 中 `SUPPORTED_ROOM_VERSIONS` 仍包含 `"12"`、`"13"` 且 `can_create: true`。`P1` 已确认为过度声明。仅默认值修复（统一为 11）**未移除** v12/v13 的 `can_create` 标志。
   - 影响：对外声称支持 v12/v13 创建，可能引入尚未完整验证的房间特性组合。

2. **MSC\* 语义分裂**（待追踪）
   - SDK fork `@langkebo/matrix-js-sdk` 2026-09-03 的实现与 Sprint 4 后端语义不一致（用户 MEMORY 记录）。`sdk-encapsulation-audit.md` 已产出但**未落入 `docs/audit/`**（文档清单中无此文件）。需确认该审计是否已过期，或补入文档清单。

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

### 当日完成（合入主干即生效）
| 优先级 | 事项 | 位置 |
|---|---|---|
| **P2** | restricted join allow 数组解析（MSC3083） | `synapse-services/src/room/membership/{service,actions}.rs` |
| **P4** | RateLimitConfig 配置漂移防护 | `synapse-common/src/config/rate_limit.rs` |
| **P0-1** | 测试池 URL 探测缓存 + 超时放宽 | `synapse-storage/src/test_utils.rs` |
| **运维** | schema 历史泄漏手工清理 + PG 参数持久化 | `synapse_test` 重建；`postgresql.conf` |

### 仍高优（按序）
1. **restricted join 单元测试补齐**（P2）— 代码已实现，待 `#[cfg(test)]` 覆盖边界（空 allow、非法 room_id、大小写、多 rule 合并）。需先修复 `test_mocks.rs` 编译（缺 `test-utils` feature）或隔离测试环境。
2. **P0-1 门禁漂散去风险**（P5）— 5 处改动在工作区未提交；CI 临时收口建议固定 `--test-threads=4`。
3. **pruning/retention 零测试**（P4）— 2,128 行代码零测试，规模与风险不匹配，建议 Sprint5 大批次补齐。
4. **v12/v13 can_create 过度声明**（P2）— 移除或加 deprecation 标记。
5. **MSC 语义分裂审计**（P2）— `sdk-encapsulation-audit.md` 未入册，需确认是否过期并纳入 `docs/audit/`。

### S 系列技术债（Sprint5 批次）
- `auth/token.rs` 4 处 `get_raw`（测试断言，语义待同步）
- clippy cosmetic（剩余 ~10 条）
- cargo doc 警告 / allow(dead_code)
- 迁移 undo 链、Sliding Sync / Space 基线

---

*本汇总基于 2026-09-12 HEAD(5fb798a4) 的文件扫描与代码 grep，并在当日 18:11 同步了工作区 5 处未提交改动与 `synapse_test` 重建事实。*
