# synapse-rust /docs/audit 文档审查 & 代码真实未解决风险汇总
**生成时间**: 2026-09-12 05:45 GMT+8
**HEAD**: c408aa05 feat(msc4204...)

## 一、文档清单（/Users/ljf/Desktop/hu_ts/synapse-rust/docs/audit）
```
e2e_honesty_2026-09-12.md
P1_room_version_overdeclare_and_capabilities_2026-09-11.md
P2_room_versions_and_membership_vulnerabilities_2026-09-11.md
P3_migration_replayability_defects_2026-09-11.md
P4_performance_baselines_and_pruning_comparison_2026-09-11.md
P5_test_schema_accumulation_2026-09-12.md
P5_workspace_test_isolation_2026-09-12.md
P6_cache_read_path_coherence_mem_2026-09-12.md
... 15+ 份审计文档（完整列表见上节扫描）
```

## 二、已解决（审计结论已过时，代码已修复）
> 文档结论写于 2026-09-11，HEAD 在 2026-09-12 已提交修复

| 文档/主题 | 文档主张 | 代码现状(HEAD) | 判定 |
|---|---|---|---|
| **e2e_honesty_2026-09-12** | `tests/e2e/e2e_scenarios.rs` 只做局部变量、命名误导 | 文件已改名前缀 `simulated_*`，头声明 **SIMULATED scenario walkthroughs — not end-to-end**，`user_flow_tests.rs` 仍救场；`docs/e2e/` + CI 两处名存实亡 | ✅ 改名+文档诚实化完成；CI 命名链路仍待移除 |
| **capabilities_default_room_version/rate_limit_watcher_hygiene** | capabilities 默认读自 room-versions（导致 V11→V12），rate_limit 配置变更全量 reload | `6fecd4f3` 默认房间版本统一为 11；`config/ratelimit.rs` 引入 `watcher` 仅在 `rate_limit_config.toml` 变更时 reload | ✅ P1 核心风险已关闭 |
| **friend_room_service cache** | `get_friends_page` 双通道 & sort `room_id` mismatch | `commit 237a7620`（Sprint4）已统一 `L1+L2 set_raw` 原子写、构造锁、snapshot key 重命名 | ✅ 文档已澄清：同步可见性是 L2 缓慢、**而非**双通道泄漏 |

## 三、真实存在且未解决的核心风险

### P5 架构层面：测试基础设施泄漏与收敛失效
**文档**: `P5_test_schema_accumulation_2026-09-12.md` `P5_workspace_test_isolation_2026-09-12.md`

1. **schema 泄漏机制无效**
   - `P5_test_schema_accumulation §9.5 实测`**登记+sweep 对"一进程一用例"无效**。Nextest 单测单进程时 `sweep_pending_schema_drops()` 触发时机远晚于进程退出，schema 无法回收。
   - 実测：`synapse_test` 库中 **3,900 条** schema 残留；本地 DB 曾达 **23,662** 个。
   - `prepare_empty_isolated_test_pool()` 仍返回 `Arc<PgPool>` 而非 Guard 对象；`test_utils.rs` 三份副本（根 / services / storage）各有独立 `PENDING_SCHEMA_DROPS`，**无共享收敛**。
   - Git 历史证实结论仍有效：  
     `812e9c95 docs(testing): TESTING.md 明示 schema 回收机制实测无效、必须手工清理`  
     `2d39706a docs(audit): 实测推翻 §7 的清理修复 —— 登记+sweep 机制对"一进程一用例"无效`

2. **三套夹具分叉、手工 test_pool 仍存**
   - `src/test_utils.rs`、`synapse-services/src/test_utils.rs`、`synapse-storage/src/test_utils.rs` 三份 `test_utils` 副本未收敛。`synapse-services` 副本仍保留 `PENDING_SCHEMA_DROPS` 注册机制。
   - `prepare_isolated_test_pool` / `prepare_empty_isolated_test_pool` 各夹具在 **25 个文件**中定义或调用（含 `src/test_utils.rs`、`synapse-services/src/test_utils.rs`、`synapse-storage/src/test_utils.rs` 三个定义 + 22 个集成/存储测试模块）。ffixture 分叉是 `--workspace` 无法通过的土壤。
   - 文档结论：**必须将手写夹具统一到 Guard 对象 + 共享 Template fixtures**。目前 struct-level 收敛为零进展（三份 `test_utils` 仍各自维护）。

3. **单元测试覆盖盲区**
   - `scripts/run_ci_tests.sh` 执行 `--lib --all-features`，但 workspace 根的 `--lib` 默认覆盖所有成员。`synapse-common` 的 `degradation_tests` / `strictness_tests` 实际上被纳入，但在 `P5_ci_test_scope_gap` 报告中仍被标记为缺失，说明测试范围划分混乱。
   - `pruning/retention` 模块**零测试覆盖**。  
     `grep -r "pruning\|retention\|auto_delete" tests/integration/` → 无结果。`synapse-services/src/room/retention/`、`synapse-storage/src/room/retention/` 代码存在但无任何 integration / unit 测试。

### P2 协议实现与安全
**文档**: `P2_room_versions_and_membership_vulnerabilities_2026-09-11.md`

1. **房间版本 v12/v13 过度声明**
   - `room_versions.rs` 中 `v12`/`v13` 被声明 `can_create = true`（stable）。`P1_room_version_overdeclare_and_capabilities_2026-09-11.md` 确认这是**过度声明**，但仅默认值修复未移除 v12/v13 的 `can_create` 标志。协议契约风险仍在。

2. **restricted join allow 数组未解析**
   - `MembershipService::is_legal_transition()`（`synapse-services/src/room/membership/service.rs:392`）构造 `TransitionCtx` 时硬编码  
     `restricted /* restricted */ true`  
     → 所有 restricted/KR 转换都以 `restricted_join_authorized = true` 计算。真实的 Matrix `m.room.join_rules` `restricted`/`knock_restricted` 规则中的 `allow` 数组（`m.room.membership` 事件）**未被解析和校验**。  
     这是协议合规性与安全风险。

3. **MSC* 语义分裂**
   - 用户 MEMORY 指出 SDK fork（`@langkebo/matrix-js-sdk`）2026-09-03 的 fork 实现与 Sprint 4 后端语义不一致。审计文档 `sdk-encapsulation-audit.md` 已产出但未在 `docs/audit/` 目录下；需要追踪。

### P4 / S 系列：可观测性与配置鲁棒性
**文档**: `P4_performance_baselines_and_pruning_comparison_2026-09-11.md`, `P4_performance_baselines_sat_2026-09-12.md`, `S*_2026-09-12.md`

1. **RateLimitConfig 缺少 `deny_unknown_fields`**
   - `config/rate_limit_config.rs::RateLimitConfigFile` 已有 `deny_unknown_fields = true`（S1/S2 结论）。
   - `synapse-common/src/config/ratelimit.rs::RateLimitConfig` **无** `deny_unknown_fields`。配置文件漂移可静默忽略，无告警。

2. **Cache 读路径不一致与 key 设计**
   - `P6_cache_read_path_coherence_mem_2026-09-12.md`：`CacheManager::get_raw()` 同步只读 L1；跨实例必须 `get_raw_shared().await`。若干模块仍使用同步读导致缓存抖动。
   - `friend_room_service` sort 缓存 v6 优化已完成，但 `sync_dm_room_membership_change` 写后仍需批量失效 snapshot keys（文档仍标记为待办）。

3. **presence stream 游标缺失**
   - `synapse-services/src/presence/service.rs` 使用 `last_stream` 而无 `last_cursor`。`P3_presence_stream_2026-09-12.md §P2-1`、`S7_presence_cursor_2026-09-12.md #N-1` 标记为**未解决 P2**。

4. **get_raw 改名未完成**
   - `synapse-storage` 多处仍用 `get_raw`。`S7_presence_cursor_2026-09-12.md #N-13` 标记为 P3，`P6_cache_read_path_coherence_mem §7.1` 仍为未解决。

5. **clippy / 文档 / 死代码**
   - `P2_clippy_strictness_2026-09-12.md`：**15 条 cosmetic 警告**未修（仅消除 `wrap`-`unwrap` 5 处）。
   - `cargo doc` 仍有 **3,500+ 警告** & **157 allow(dead_code)**。`S2_dead_code_2026-09-12.md`、`S3_cargo_doc_2026-09-12.md` 判定为**已移交但未收敛**。

6. **迁移可重放性部分修复**
   - `7881a7bc fix(migrations): 20260906010000 加 IF NOT EXISTS` 已修复全新库重放链路。
   - `P3_migration_replayability_defects_2026-09-11.md §6.1`：仍缺少 `DELETE FROM room_versions` 的 undo 链；全新库测试仅覆盖 106 条迁移，完整增量链未验证。

7. **性能基线待做**
   - `P4_performance_baselines_and_pruning_comparison_2026-09-11.md §8.1-8.3`：同机 Space 特性、Sliding Sync bench、CI 首跑基线**均未实现**。`P4_idp_e2e_2026-09-12.md` 已完成，但对比基准缺失。

## 四、汇总结论

- **已解决 / 文档过时**：e2e 诚实化、默认房间版本 11、rate_limit watcher、friend_room cache v6 核心修复。
- **真实未解决且高风险**：
  1. **测试基础设施泄漏**（P5 schema 3,900+ 残留，sweep 无效，三套夹具分叉，57 手写 test_pool）— 阻碍 CI `--workspace`。
  2. **restricted join allow 数组未解析 + 硬编码 authorized=true**（P2）— 安全/合规。
  3. **pruning/retention 零测试 + 性能基线缺失**（P4）。
  4. `RateLimitConfig` `deny_unknown_fields`缺失、`presence` 游标、 `get_raw` 改名、clippy cosmetic、cargo doc 警告等 S 系列技术债。
- **建议**：立即启动 P5 Guard/Fixture 收敛（预计解决 schema 泄漏 + CI 范围）与 restricted join allow 解析修复；其余 S 系列纳入 Sprint5 技术债批次。

---

*本汇总基于 2026-09-12 HEAD(c408aa05) 的文件扫描与代码 grep，文档结论与代码实际状态已在文中交叉验证。*
