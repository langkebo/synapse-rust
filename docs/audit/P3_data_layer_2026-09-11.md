# P3 — 数据层与持久化

> **审查日期**: 2026-09-11
> **基线**: `baa73bc4` 之前（`e6f0eb4b`），工作树对本次审查干净
> **范围**: schema 单一真相源 · 迁移可重放 · N+1 · 事务边界 · 时间类型一致性

---

## 0. 结论摘要

| 子项 | 状态 | 证据 |
|---|---|---|
| schema 单一真相源 | ✅ **已闭环**（本会话） | `2b16dc3c` + `ca2d65c9` + `506e41a6` |
| 迁移可重放 | ✅ **已验证** | 全新库 `applied=38`；重复执行 `applied=0`（幂等） |
| undo 覆盖 | ✅ **已闭环** | 36 forward ↔ 36 undo；CI 检查 `EXIT=0` |
| 时间类型一致性 | ✅ **已验证通过**，无残留缺陷 | 见 §2 |
| **N+1** | 🔴 **发现并已修复 1 处** | `/sync` invited-room 路径，见 §3 |
| 事务边界 | ✅ 抽查未发现缺陷 | 见 §4（有限范围，非全量） |

---

## 1. schema 单一真相源 / 迁移可重放（已完成，引用前序提交）

| 项 | 结果 |
|---|---|
| 权威目录 | `migrations/`（**唯一**）；`docker/deploy/migrations` 死副本已删除（191 文件） |
| deploy 路径 | `docker-compose.yml` 直接挂载 `../../migrations:/migrations:ro` |
| 全新库 migrate | 基线候选数=1（v11），`applied=38, skipped=0`，public 表数 253 |
| **幂等性** | 同一库第二次 migrate → `applied=0, skipped=1` |
| `validate` | 数据库架构验证通过 |
| baseline consolidation | `EXIT=0`（此前报 `events.soft_failed` 漏折入） |
| migration consistency | `status=ok, issues=0, warnings=0`（此前 13 issues + 42 warnings） |

> ⚠️ **undo 的语义边界**（已文档化，非缺陷掩盖）：
> `normalize_room_alias_server_name` 的数据小写化**真正不可逆**，
> `add_events_soft_failed` 的删除会让 soft-failed 事件重新可见 —— 二者的 undo 为
> "说明 + 受控操作"而非机械反转，符合仓库既有惯例。

---

## 2. 时间类型一致性 —— ✅ 已验证通过

**背景**：v10 baseline 做过 `TIMESTAMPTZ → BIGINT`（毫秒）大迁移，需确认无残留混用。

### 2.1 baseline 层面

```
grep -c "TIMESTAMPTZ" migrations/00000000_unified_schema_v11.sql  →  1
```
该唯一命中是**版本历史注释**（`-- v10.0.0: 折入 TIMESTAMPTZ→BIGINT 统一修复`），
**不是列定义** ⇒ schema 声明层面无 `TIMESTAMPTZ` 残留。

### 2.2 v10 遗留的两个列已修复

`20260711120000_fix_device_trust_timestamptz_to_bigint.sql` 记录了 v10 漏掉的两列。
核对 baseline 现状：

| 列 | baseline 类型 | 判定 |
|---|---|---|
| `device_trust_status.verified_at` (schema 第 707 行) | `BIGINT` | ✅ 已统一 |
| `cross_signing_trust.trusted_at` (schema 第 719 行) | `BIGINT` | ✅ 已统一 |
| `user_privacy_settings` 系列 (第 1421 行) | `BIGINT` | ✅ 已统一 |

该修复迁移使用 `information_schema` 守卫（仅当仍为 `timestamp with time zone` 才转换）
⇒ 幂等，对已迁移库为空操作。

**结论：时间类型一致性无遗留缺陷。** 且与项目规则一致（`*_ts` = BIGINT NOT NULL 毫秒；
`*_at` = BIGINT 可空毫秒）。

---

## 3. 🔴 N+1 —— 发现并已修复 1 处（`/sync` 关键路径）

### 3.1 缺陷

`synapse-services/src/sync_service/response.rs` 的 invited-room stripped state 构建：

```rust
// 调用方（原第 128-132 行）：per-room 循环
for room_id in &rooms_to_include {
    if room_sections.get(room_id) == Some(SyncRoomSection::Invite) {
        let stripped = self.build_invited_room_stripped_state(room_id, user_id).await;  // ← N 次
        ...

// 被调函数（原第 542 行）：再循环 8 种 state 类型
for event_type in STRIPPED_STATE_TYPES {                       // 8
    let state_events = self.event_reader
        .get_state_events_by_type(room_id, event_type).await.ok()?;   // ← 8×N 次
```

**⇒ 单个 `/sync` 内 `8 × N` 次查询**（N = 该用户被邀请的房间数）。
一个被邀请到 10 个房间的用户，一次 sync 触发 **80 次** state 查询，全部在
`/sync` 热路径上（sync 是最频繁的客户端请求）。

### 3.2 同时存在的错误语义缺陷

`.ok()?` 把**查询错误**折叠为 `None`：

- 任何一次查询失败 ⇒ 整个函数返回 `None` ⇒ 该房间**完全从 invite 段消失**
  （客户端看不到邀请，用户无法加入）
- 失败路径**没有任何日志** —— 仅当 `m.room.create` 恰好缺失时才有一条 warn，
  而 DB 错误本身不可观测

### 3.3 修复

| 改动 | 效果 |
|---|---|
| 调用方先收集 invited room ids | — |
| 新增 `build_invited_rooms_stripped_state(&[room_id])` | 用**已存在**的 `get_state_events_by_type_batch` 预取 ⇒ 查询数 **8×N → 8**（与房间数解耦） |
| 抽出纯函数 `assemble_invited_room_stripped_state(events, user_id)` | MSC4311 fail-closed 规则可**直接单测**（无 I/O） |
| 错误语义显式化 | 某 state type 批量读失败 → 该类型降级为"缺失" + **warn 记录**（不再静默折叠整个 stripped state） |
| 删除已无调用方的逐房间函数 | 消除死代码 |

> **保持的语义**：缺少 `m.room.create` 的房间仍按 MSC4311 fail-closed 从 invite 段省略。
> 行为等价，但查询数从 `8×N` 降为常量 8，且失败可观测。

### 3.4 回归测试（新增 3 个，全通过）

| 测试 | 保护对象 |
|---|---|
| `stripped_state_is_none_without_create_event` | fail-closed：无 `m.room.create` ⇒ `None` |
| `stripped_state_is_some_with_create_event` | 正向组装包含 create 与 name |
| `stripped_state_keeps_only_the_invitee_member_event` | **防成员列表泄漏**：仅保留被邀请者自己的 `m.room.member` |

> ⚠️ **诚实标注**：现有测试基建没有可计数的 `EventReader` mock，
> 因此**查询次数本身未被断言** —— 上述测试保护的是**行为**（含安全语义），
> 而非"不回归成 N+1"。彻底的 N+1 守卫需要一个 counting mock，属后续基建项。

---

## 4. 事务边界 —— 抽查未发现缺陷（范围有限）

抽查了前序审查标记的 `admin_user_service` 批量踢人路径
（`admin_user_service.rs:340-394`）：

| 检查点 | 结果 |
|---|---|
| 并发是否受控 | ✅ `buffer_unordered(self.evict_max_concurrency)` 限并发 |
| 批量 decrement 失败 | ✅ **已显式上报**（推入 `failures`），非静默吞掉（B-1.1 已修） |
| 逐房间失败 | ✅ 收集为 `AdminEvictionFailure { room_id, error }` |

> ⚠️ 本项为**抽查**，非全量审查。未系统检查所有跨表写入是否被事务包裹、
> 部分失败是否留下不一致状态。列为后续项。

---

## 5. 顺带修正：一个固化了已删除缺陷的测试

`tests/unit/migration_consistency_tests.rs` 的
`test_v11_primary_and_deploy_migrations_match` 断言：

```rust
let deploy_baseline = deploy.join("00000000_unified_schema_v07.sql");
assert!(deploy_baseline.exists(), "missing deploy v7 baseline");
```

即该测试**把手工同步副本（漂移根源）当作期望状态固化下来**。在 `2b16dc3c`
删除副本后它变红，**正确暴露了这个测试本身的错误**。

已改写为 `deploy_mounts_canonical_migrations_and_has_no_copy`，验证新契约：
1. canonical v11 基线存在
2. `docker/deploy/migrations` **不得存在**（副本或符号链接均不可）
3. `docker-compose.yml` 确实挂载 `../../migrations:/migrations`

---

## 6. 验证汇总

| 验证 | 命令 | 结果 |
|---|---|---|
| lib + unit 全量 | `cargo nextest --profile test --features test-utils --lib --test unit` | ✅ **2434 passed, 0 failed** |
| 新增 stripped-state 测试 | `--lib stripped_state` | ✅ 3 passed |
| migration consistency | `python3 scripts/check_migration_consistency.py` | ✅ `status=ok, issues=0` |
| baseline consolidation | `python3 scripts/check_baseline_consolidation.py` | ✅ `EXIT=0` |
| 迁移幂等性 | 同一库第二次 migrate | ✅ `applied=0` |
| fmt | `cargo fmt --all -- --check` | ✅ PASS |
| clippy | `cargo clippy -p synapse-services --all-features -- -D warnings` | ✅ `EXIT=0` |

> 所有验证均使用 `CARGO_TARGET_DIR=/tmp/p3t` 隔离，规避与并发 agent 的
> `target/` 写冲突（见 P5 §3.1）。

---

## 7. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export CARGO_TARGET_DIR=/tmp/p3t      # 隔离并发构建

# N+1 修复的回归测试
cargo nextest run --profile tdd --features test-utils -p synapse-services --lib stripped_state

# 全量
cargo nextest run --profile test --features test-utils --lib --test unit

# 迁移相关门禁
python3 scripts/check_migration_consistency.py
python3 scripts/check_baseline_consolidation.py

# 时间类型一致性（唯一命中应为版本历史注释）
grep -n "TIMESTAMPTZ" migrations/00000000_unified_schema_v11.sql
```

---

## 8. 移交后续

| 项 | 说明 |
|---|---|
| counting `EventReader` mock | 让"不回归成 N+1"可被断言（当前只能断行为） |
| 全量事务边界审查 | 本次仅抽查 `admin_user_service`；未覆盖所有跨表写入 |
| 其他 N+1 候选 | `sliding_sync_service/filters.rs:130`（循环内 `storage.get_room`）等未逐一判定 |
