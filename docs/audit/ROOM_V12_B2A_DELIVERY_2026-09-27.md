# Room v12 B2a 交付与并发写者事故记录（2026-09-27）

- **工作树**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.worktrees/room-v12`
- **分支 / HEAD**：`feat/room-v12-complete` @ `8cc2ad00e`
- **上游计划**：`docs/audit/ROOM_V12_COMPLETION_PLAN_2026-09-27.md`
- **本轮性质**：**执行记录**。只改生产代码与提交信息，未改计划文档，未联网。

> **命名说明（避免与计划文档的阶段号混淆）**：本文的 `B2a` / `B2b` / `B3` 是**本轮任务书**
> 的分段名，**不是**计划文档 §3.2 的 `A-1`/`B-1`/`C-1`… 编号。对应关系见 §5。

---

## 1. 本轮交付（3 个提交，均在工作树上，未 push）

| 提交 | 内容 | 触及 |
|---|---|---|
| `6036c4cb8` | **B2a part 1**：无域名 room ID 单语法实现 + 6 处联邦守卫 | `synapse-common/src/room_id.rs`（新增）、`validation.rs`、`synapse-web/src/routes/validators.rs`、6 个联邦 handler |
| `b089e0323` | **B2a part 2**：房间本地性由**归属记录**而非 id 拼写决定 | `synapse-services/src/room/membership/{service,actions,federation}.rs` |
| `8cc2ad00e` | **B2a part 1 修补**：`thirdparty_invite` 守卫的多余借用（clippy `needless_borrow`） | `synapse-web/src/routes/federation/membership/invite.rs:25`（1 行） |

### 1.1 part 2 的实现要点

- 新增 `MembershipService::room_locality` 作为"本服务器是否托管该房间"的**唯一实现**（铁律 2）：
  1. `m.room.create` 状态事件的创建者所在服务器（v1–v10 读 `content.creator`，v11+ 读事件 sender，
     与 `AuthService::resolve_room_creator` 同序）；
  2. 仅当没有 create 事件时，回落到 `rooms.creator_user_id`。
- `rooms` 行的 creator 只是**回落**、不能作主信号：走联邦 join 的房间该列记的是**本地**加入者
  （`join_room_via_federation`），单看它会把所有联邦房间判成本地。
- id 解析助手改名为 `user_server_name` / `is_remote_user_id` / `is_remote_user`（用户 ID 恒含 `:server`）；
  `server_name_from_id` / `is_remote_id` / `is_remote_room` **删除**（铁律 1，不留兼容残留）。
- **Fail-closed**：无归属记录的房间判为 **remote**（`destinations` 可为空，此时调用方必须失败，
  不得回落本地路径）。`RoomLocality::Remote { destinations }` 携带居民服务器列表
  （origin 优先，其后是本地已知的有加入成员的服务器，去重、排除本机）。
- `leave_room` / `leave_and_forget` 改用该判定。远程 leave 按居民列表依次尝试，**只在
  `BadRequest` / `Forbidden` 上重试** —— 这两类产生于 ACL 检查、make_leave、模板校验、send_leave，
  都在任何本地写入**之前**，重试不会破坏本地状态；其它错误（签名/本地持久化，发生在远程交换**之后**）
  立即上报，不换下一个居民重放。

---

## 2. 并发写者事故（AGENTS 铁律 9 的第四次实例）

本轮在**同一工作树**上观察到另一个写者的连续动作，全部发生在 `6036c4cb8`（20:42）之后：

| 时间 | 事件 |
|---|---|
| 21:30:09 | 另一写者 `git stash` 走了 part 2 的三个未提交文件（stash 名 `b2a2-wip`），工作树一度"变干净"、文件回到 part 1 旧实现 |
| 21:32 | 本会话核对 stash 内容后 `git stash pop stash@{0}` 恢复（pop 无冲突，内容 sha1 一致） |
| 21:35–21:37 | 另一写者做了一次 `git stash pop`，在 `synapse-common/src/lib.rs`、`synapse-storage/src/lib.rs`、`synapse-web/src/middleware/security.rs` 留下 `UU`，并带两个 `DU` 外来路径 `src/web/mod.rs`、`src/web/routes/extractors/mod.rs`（本仓布局是 `synapse-web/`，HEAD 树里 `src/web/` **0 命中**）；期间本会话的一次变异探针因此编译失败 |
| 21:37:50 | 该写者**自行解决**冲突（`git ls-files -u` 清空、`src/web/` 从磁盘消失） |
| 21:38 | 本会话以 `git commit --only <3 路径>` 落盘 part 2（不动索引里的冲突态、不夹带他人改动） |

**三方一致性核对**（另一写者留在 `/tmp/room-v12-locality-backup/` 的副本 vs 本会话提交）：

```
actions.rs    IDENTICAL HEAD == user-backup == worktree (633e6fa772a772ae646db539cbb14420028c5c2b)
federation.rs IDENTICAL HEAD == user-backup == worktree (fbdaee79f8b26141943d19b308bb2b5efbfa09f9)
service.rs    IDENTICAL HEAD == user-backup == worktree (9093831b26e84adf6d34acf942cb91486d68174a)
git apply --check --reverse /tmp/room-v12-locality-backup/locality.patch  -> patch matches HEAD exactly
```

⇒ 没有两个版本的分歧，**无需** `git rm --cached src/web/*`，也**无需**用备份覆盖（那些动作在 21:38 之后
已成空操作）。`src/web/` 在当前 HEAD 与磁盘上都不存在（`git ls-tree -r --name-only HEAD | grep -c '^src/web/'` = 0）。

**教训（与既有铁律一致）**：`git add -A` / `git stash` / `git stash pop` 都作用于**整棵工作树**，
在"同一工作树两个写者"下必然互相卷带。本轮 part 2 一度只存在于 stash 中，若不是本会话先核对
stash 内容再恢复，就可能被后续操作丢失。**该工作树在完成本目标前应保持单写者**。

---

## 3. 门禁结果（实跑证据）

| 门禁 | 命令 | 结果 |
|---|---|---|
| fmt 棘轮 | `./scripts/check_fmt_ratchet.sh` | **PASS**：current=0 baseline=0 |
| workspace 编译 | `cargo check --workspace --all-targets --all-features --locked` | **EXIT=0** |
| clippy tier1 | `cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings` | **仅剩既有红项**：`synapse-storage/src/invite_blocklist.rs:410` 死代码 `cleanup_allowlist_by_suffix` |
| clippy tier2 | 同上 `--all-features` | **仅剩既有红项**：同上 + `tests/integration/api_invite_blocklist_e2e_tests.rs:48` 未使用变量 `admin_token` |
| 相关单测 | `cargo nextest run -p synapse-web --lib --features test-utils -E 'test(validators) \| test(room_id)'` | **33/33 passed** |
| 相关单测 | `cargo nextest run -p synapse-services --lib --features test-utils -E 'test(/membership/)'` | **126/126 passed** |
| 全量 lib（非 DB） | `cargo nextest run -p synapse-services --lib --features test-utils --test-threads 4 --no-fail-fast -E 'not test(database_initializer)'` | **1826/1826 passed**，32 skipped |
| `.sqlx --static` | `bash scripts/ci/check_sqlx_cache_fresh.sh --static` | **OK**（1162 条，已被 git 跟踪） |
| `.sqlx --compile` | `bash scripts/ci/check_sqlx_cache_fresh.sh --compile` | **OK**（离线构建通过，缓存覆盖全部调用点） |
| SQLx 棘轮 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | **FAIL（既有红项）**：production 287>275、test 718>716、static 1192<1193 |

**既有红项的归属已用 stash 探针确认**：把本轮改动 stash 掉后在同一 HEAD 上重跑，
`check_sqlx_dynamic_ratio.sh` 的三项违规**逐字相同**；clippy 的两项也与本轮无关
（invite-policy 面的死代码与测试变量，均属并发会话）。

> **关于全量 lib 批次**：直接跑会遇到 `database_initializer` 的首个失败
> （`TEST_DATABASE_URL must be set ... NotPresent`）并触发 nextest 的 fail-fast，
> 1858 个用例只跑了 493 个。要拿到本模块的完整信号，需加 `--no-fail-fast` 并排除该模块
> （或设置 `TEST_DATABASE_URL`）。这属**环境性失败**，不是代码回归。

---

## 4. 变异自证（铁律 8）

把 `room_locality` 临时改成恒返回 `RoomLocality::Local`（即 part 1 之前的旧行为），
用**新的 6 个用例**跑：

```
Summary [0.020s] 6 tests run: 2 passed, 4 failed, 1852 skipped
FAIL domainless_remote_room_leave_room_routes_to_federation
FAIL leave_and_forget_refuses_domainless_remote_room
FAIL room_locality_lists_the_remote_residents
FAIL unknown_domainless_room_fails_closed_to_remote
   （fail-closed 用例的断言输出：left: Local / right: Remote { destinations: [] }）
```

变红的 4 个正是"本地性判定"直接决定结果的用例；通过的 2 个是**阳性对照**
（`domainless_local_room_stays_local`，Local 是它的期望值）与命名无关的辅助用例
（`domainless_room_id_has_no_parseable_server`）。随后 `git checkout HEAD -- <file>` 复原，
`git status` 0 改动。

---

## 5. 与计划文档的对应关系

| 本轮分段 | 内容 | 计划文档条目 | 状态 |
|---|---|---|---|
| B2a part 1 | 无域名 room ID 语法 + 6 处联邦守卫 | §C-3 第 1/2/3 项（`validation.rs`、`validators.rs`、6 处联邦硬校验） | ✅ 已落盘 |
| B2a part 2 | 房间本地性由归属记录决定 | §C-3 第 5 项（G-21）+ §D-6 目的地语义的前置 | ✅ 已落盘 |
| **B2b** | **MSC4291 创建侧：create 事件身份 finalize + `$`→`!` room_id 推导与流程重排** | **§C-1（C-1 的前半）+ §C-2** | ⏳ 未开始 |
| **B3** | **MSC4291 房间升级/迁移侧：`predecessor.event_id` 废弃、升级顺序反转** | **§C-4（G-14/G-30）+ §C-1 的收尾** | ⏳ 未开始 |
| B4 → B7 | §D（入站规则 1.2/2/2.5）、§E（MSC4289）等 | 计划 §D / §E | ⏳ 未开始 |
| C | §F MSC4297（含 F-1 "零调用者"接线决策） | 计划 §F | ⏳ 未开始 |

**B2b/B3 的硬前置是 §C-3 第 4 项（DB CHECK 放宽）**，它是"B2b+B3"内部的第一步、
不是独立分段：`ck_rooms_room_id_format` 仍是
`CHECK (room_id ~ '^![a-zA-Z0-9._=+./-]+:[a-zA-Z0-9.-]+$')`
（`migrations/00000000_unified_schema_v12.sql:4709-4715`，注释仍写"`!opaque:domain`"），
无冒号 room id 触发 **23514**，建房必败。不做它，B2a 放行的语法到 DB 层就被截断。
连带面（**实测**，接手时直接照此清单走）：

| 连带项 | 位置 | 说明 |
|---|---|---|
| 约束本身 | `migrations/00000000_unified_schema_v12.sql:4709-4715` | `ADD CONSTRAINT` 前需 `DROP CONSTRAINT IF EXISTS` —— 约束改名不可原地改（本仓已踩过"`CREATE OR REPLACE` 不能改参数名"同型坑） |
| 指纹基线 | `tests/unit/test_isolation_unification_tests.rs:151` | `EXPECTED_BASELINE_FINGERPRINT = "efd39fc561affd7a"` 必须重算（R10 第①条：先用旧值自检哈希实现，再取新值） |
| 断 schema 的契约用例 | `tests/unit/msc_tests.rs`、`tests/integration/invite_blocklist_tests_migrated.rs` | 两文件含 `room_id_format` 断言，必须同步（R10 第③条；漏改 3 条曾让 CI 集成批次必红） |
| B2b 本体证据点 | `create_events.rs:42`（`generate_event_id` 占位 ID）、`graph_metadata.rs:490-511`（`create_event_with_graph` 不 finalize 的旁路） | §C-1 的"本地创建 vs 入站持有 origin 图数据"必须分流，不能统一 finalize |
| B3 本体证据点 | `service.rs:24-26`（`config.room_id` 抢占式逃逸口）、`:549-554`（`predecessor.event_id` 写入） | §C-2 要先把 `rooms` 行写入排到 create 事件定稿**之后** |

---

## 6. 剩余面上仍未处理的 room_id 冒号解析（实测，供 B2b/B3 接手）

生产代码里仍按 `:` 反推服务器的位置（`grep` 实测，本轮未动）：

| 位置 | 现状 | 计划条目 |
|---|---|---|
| `synapse-web/src/routes/federation/membership/invite.rs:277` | `format!("${}:{}", uuid, room_id.split(':').next_back().unwrap_or("server"))` —— 无冒号时 `next_back()` 返回整串 `!xxx`，产出 `$uuid:!xxx` 畸形事件 ID | §C-3 第 6 项（G-22） |
| `synapse-services/src/room/membership/actions.rs:41` | join 目的地 `room_id.rsplit_once(':')` 回落（`via_servers` 为空时对无冒号 id 得 `None`，当前会 400，不是静默错误，但语义仍依赖拼写） | §C-3 第 5 项（G-21 的 join 侧） |
| `synapse-storage/src/space/repository.rs:30` | `request.room_id.split(':').next_back().unwrap_or("localhost")` | §C-3 第 6 项 / G-16 |

已收敛的验收判据（实测）：

```
grep -rn "room_id.contains(':')" <5 个生产 crate>   -> 0（part 1 前是 6 处）
validate_room_id 调用点                            -> 117（计划文档记为 110，已漂）
```

---

## 7. 下一步建议（按依赖排序）

1. **B2b/B3 的第一步 —— DB CHECK 放宽**：把 `ck_rooms_room_id_format` 改为"两种形态任一"
   （或删除该 CHECK，形态校验已在 `synapse-common::room_id` 单点），先 `DROP CONSTRAINT IF EXISTS`；
   按 R10 重算 `EXPECTED_BASELINE_FINGERPRINT`、跑 `test_isolation_unification`、
   同步 `msc_tests.rs` / `invite_blocklist_tests_migrated.rs` 的 schema 断言；
   配套 `.sqlx` 两道 + 两档 clippy（R8）。
   **门禁须自证能变红**：用真 baseline DB 往返插入一个无冒号 room id，确认 23514 消失。
2. **B2b**：§C-1 的 create 事件身份 finalize（注意 `create_event_with_graph` 同时服务入站，
   不能统一 finalize）→ §C-2 的 `$`→`!` room_id 推导与创建流程重排。
3. **B3**：§C-4 的 `predecessor.event_id` 废弃与升级顺序反转。
4. 之后再进 §D（入站规则）、§E（MSC4289）、§F（MSC4297，先做 F-1 的接线决策）。

**协作要求**：本工作树同时只能有一个写者；需要并行时按计划 §3.3 另开 worktree
（并各自使用私有 `CARGO_TARGET_DIR`）。
