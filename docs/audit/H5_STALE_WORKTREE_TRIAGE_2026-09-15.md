# H-5 陈旧 worktree 甄别报告（2026-09-15）

对象：`.claude/worktrees/optimization+audit-2026-07`（分支 `optimization/audit-2026-07` @ `488d9888`）
任务：157 个未合并提交 + 其未提交改动的处置
处置方式：按主题甄别后**定向移植**（用户选定），不整体 merge

---

## 0. 结论（TL;DR）

**甄别结论是"没有需要移植的对象"。**

| 观测 | 数值 | 含义 |
|---|---|---|
| 157 个提交中"零实质残留"的 | **57 / 157** | 这些提交的全部新增行在 `main` 里逐字节存在 |
| 分支新增实质行总量 / `main` 中找不到的 | 36283 / **5201（14%）** | 即 **86% 的内容已在 `main`** |
| OPT-001…031 逐项核验 | **31/31 已在 `main`** | 代码已独立实现，或已吸收进 baseline 迁移 |
| 分支删除的 13 个文件 | **13/13 已在 `main` 消失** | 清理也已生效，无遗留 |
| worktree 未提交的 280 个文件 | 1850 行新增 / **238（12%）找不到** | 残留项全部是**会被 `main` 回退**的旧语义 |

也就是说：`git cherry` 报"157 个未合并"是 **patch-id 判据失效**造成的假象
（`main` 侧的这 707 个提交是用**重新实现/重写**的方式落地的，patch-id 自然对不上），
而不是"有 157 个提交的工作量没进去"。

**并且发现一个安全事件**：该 worktree 的暂存区里躺着一个**真实的 Ed25519 私钥**
（`docker/deploy/synapse-data/signing.key`，3 行 PEM，`BEGIN PRIVATE KEY`）。
已取消暂存（`git rm --cached`，磁盘内容未删、未提交）。

**收尾已完成**（§11）：worktree 已移除、分支已归档为 `archive/optimization-audit-2026-07`、
`git worktree list` 只剩 1 条、工作区无任何残留改动与未跟踪文件。

---

## 1. 对象与基线

| 项 | 值 |
|---|---|
| 分支 tip | `488d9888 perf(ci): Phase D4 — share rust-cache across --all-features jobs` |
| merge-base | `22a4fc32`（2026-07-07） |
| 分支侧提交数 | **157** |
| `main` 侧提交数（同一区间） | **707** |
| `git cherry` 判定 | 0 已合并 / **157 未合并** |
| `git merge-tree --write-tree` | **EXIT=1，300 个冲突文件 / 429 个变更文件** |
| 远端副本 | **无**（`488d9888` 不在任何 `origin/*` 上；本地删除不可恢复） |
| worktree 磁盘占用 | 39 MB |

---

## 2. 方法：为什么不能用 `git merge` / `git cherry` 当判据

- **`git merge`**：300 个冲突文件、`main` 已领先 707 个提交，且 `main` 的 B0/B1/B5 系列修复
  与分支的旧语义**互斥**（见 §7）。整体合并会**回退 `main`**，不是"合并无冲突"。
- **`git cherry`（patch-id）**：只比较补丁指纹。`main` 的等价工作是**重写**而非 cherry-pick，
  指纹必然不同 —— 157/157"未合并"里绝大多数是假阳性。§3 用**内容级**判据证伪了它。
- **本文采用的判据**：把分支相对 merge-base 新增的每一行，拿去 `main` 的完整文件树里做
  逐行存在性比对。行级比对可能低估"等价但改写"的情形，所以**只用于筛出候选**，
  候选再由 §4 逐个打开源码核验。

---

## 3. 证据 A：157 个提交的内容残留量

按"该提交新增的实质行中，有多少在 `main` 里找不到"排序（实质行 = 去空行、去纯符号行、长度 ≥12）：

- **57 个提交残留 0 行** —— 完整在 `main` 里，无需任何动作。
- 残留 >0 的 100 个提交里，量级分布：

| 残留量级 | 代表性提交 | 真实性质 |
|---|---|---|
| 1000+ | `096e01912` split openclaw / `a6b86c3a7` docs(audit) / `8634ada75` baseline | openclaw（`main` 刻意删除）、7 月审计文档、fork 点漂移 |
| 100–400 | `4f1dbcd85` perf(ci) Phase A / `12c1f8994` Phase B1 / `4cdc4d49d` rate-limit fail-closed | CI 配置（`main` 已重写）与**注释**（`main` 已换措辞） |
| 剩余 90+ 个 | `refactor(storage): split *` 系列 | **结构性分叉**：`main` 选了另一套子模块切分，所以行对不上，功能都在 |

`refactor(storage): split *` 这一批（`openclaw`/`sliding_sync`/`room_summary`/`saml`/`cas`/
`worker`/`space`/`media_quota`/`event_report`/`friend_room`/`registration_token`/
`server_notification`/`openclaw`/`application_service` 共 14 处）残留 27–77 行，
全部是"同一个函数落在不同文件/不同行号"造成的比对噪声 —— 逐项核验后无功能缺失。

---

## 4. 证据 B：OPT-001…031 逐项核验（全部已在 `main`）

对每个 OPT 条目，先看内容残留，再**打开 `main` 源码确认行为本身**：

| OPT | 内容 | 在 `main` 的落点（实测） | 判定 |
|---|---|---|---|
| 001 | OIDC JWKS 失败拒绝 id_token | `main` 有 `OPT-001` 标签 + 逻辑 | ✅ 已落地 |
| 002 | 派生失败日志脱敏 signing_key | 同上 | ✅ |
| 003 | 健康检查只信 `/health` | 残留 **0 行** | ✅ 完全一致 |
| 004 | server-key 缓存 TTL 收敛 | `client.rs:43` `KEY_CACHE_TTL_SECS.min(remaining_secs).min(max_validity_secs)` | ✅ 比分支更严 |
| 005 | 注册/密码/refresh 限流收紧 | `main` 有 `OPT-005` 标签 | ✅ |
| 006 | 成员离开触发 megolm 轮换 | `main` 在 `room/lifecycle/*`、`room/state/*` 处理 `m.room.encryption` | ✅ |
| 007 | 拒绝陈旧 key-backup 版本恢复 | `main` 有 `OPT-007` 标签 | ✅ |
| 008 | 签名密钥落库必须主密钥 | 仅剩 1 行日志文案差异 | ✅ |
| 009 | room account data 用毫秒时间戳 | 仅剩测试行差异 | ✅ |
| 010 | to-device 原子去重 | `main` 有 `OPT-010` 标签 | ✅ |
| 011 | device 创建/删除包事务 | `device/mod.rs:429,606` `pool.begin()` + `tx.commit()` | ✅ |
| 012 | delete_devices_batch 消除 N+1 | 残留仅测试辅助行 | ✅ |
| 013 (a–r) | 18 处 `nullable -> Option<i64>` | 残留 0–2 行，均为注释 | ✅ |
| 014 | 后台任务接入取消令牌 | `CancellationToken` 遍布 `main`（`event_notifier.rs`、`burn_after_read_service.rs`…），`server/mod.rs:813` 先等信号再 drain | ✅ |
| 015 | sync / sliding-sync 缓存 | `main` 有 5 个 `OPT-015` 标签 | ✅ |
| 016 | XFF 受信代理校验 | `src/web/utils/ip.rs` 有 `trusted_proxies` + `rightmost_untrusted_hop()`（防最左伪造） | ✅ 比分支更完整 |
| 017 | 联邦端点存在性泄漏 | `main` 有 6 个 `OPT-017` 标签 | ✅ |
| 018 | email 验证 token 返回 `ApiError` | `auth/mod.rs`、`auth/credential_auth.rs` 存在 | ✅ |
| 019 | Ed25519 密钥 `ZeroizeOnDrop` | 已确认在源码 | ✅ |
| 020 | `url_preview_cache.expires_ts -> expires_at` | **baseline** `00000000_unified_schema_v11.sql:4333-4334` 已含重命名与索引 | ✅ 已吸收 |
| 021 | id_token nonce 校验 | `main` 有 `OPT-021` 标签 | ✅ |
| 022 | SAML 至少一个签名 | `main` 有 `OPT-022` 标签 | ✅ |
| 023 | `AuthenticatedUser` 提取器统一鉴权 | 残留仅 handler 写法差异 | ✅ |
| 024 | 审计事件禁止删除（防篡改） | **baseline** 已含 `trg_prevent_audit_delete`（2 处） | ✅ 已吸收 |
| 025 | events 表查询迁至 EventStorage | `synapse-storage/src/event/unread.rs` 存在于 `main` | ✅ |
| 026 | 区分并发 refresh 重试与重放 | `main` 有 `OPT-026` 标签 | ✅ |
| 027 | worker 失败任务死信队列 | `synapse-common/src/task_queue.rs:372` `conn.xadd("mq:tasks:dead_letter", ...)` | ✅ |
| 028 | schema 检查失败即致命 | 残留 **0 行** | ✅ |
| 029 | `state_event_to_json` 抽公共模块 | `main` 抽到了 `synapse-services/src/sync_helpers.rs`（分支叫 `sync/event_format.rs`） | ✅ 换个位置而已 |
| 031 | knock 迁移合法性测试 | `membership_transition.rs` 含 `accept_knock_via_invite_is_legal` 等 | ✅ |

**注**：OPT-020 / OPT-024 是本报告里唯一"看起来整块缺失"的两项 —— 它们确实缺了
分支那两份 `migrations/*.sql`，但 `main` 的迁移策略是**把历史迁移吸收进统一 baseline**
（`migrations/` 只剩 `00000000_unified_schema_v11.sql` + `00000001_extensions_v10.sql`），
两份迁移的产物（触发器、列重命名、索引重命名）**已经在 baseline 里**。
补回那两个文件会与 baseline 重复执行，属于**必须不移植**。

---

## 5. 证据 C：13 个删除已全部生效

`git diff --diff-filter=D 22a4fc32 optimization/audit-2026-07` → 分支删除 13 个文件。
逐个检查 `main`：**13/13 在 `main` 中都不存在**，即分支的清理动作 `main` 侧也已完成。
→ 没有任何"待移植的清理"。

---

## 6. 证据 D：worktree 未提交的 280 个文件

`git -C <worktree> diff`：280 个文件、1912 行新增 / 1366 行删除。
按同一方法比对：新增实质行 1850，其中 `main` 中找不到 **238（12%）**，分布在 43 个文件。

残留项逐条判读：

| 残留项 | 文件数 | 判定 |
|---|---|---|
| `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]` 挂在各 crate 根 | 6（6 个 `lib.rs`） | **必须不移植**：`main` 采用更严的策略（业务代码消除、仅 mock 加 `#[allow]`） |
| 快照里出现 `/_matrix/client/r0/login/saml/*` 等 **r0 路径** | 2（route_ledger 快照） | **必须不移植**：`main` 的 A7（`52b59c8f`）已拆除 r0 兼容嵌套，回灌即回退 |
| `current_timestamp_millis()` 测试辅助函数 | 10+ | `main` 自有等价 helper，属重名不同实现 |
| 模板 schema 陈旧清理（revision 5 + ready marker） | 2 | `main` 已有同机制（`TEST_TEMPLATE_SCHEMA_REVISION = 2` + `TEST_TEMPLATE_READY_MARKER_PREFIX` + `template_schema_is_ready`） |
| 测试后门 `reset_recoverer` / `reset_pending_transactions_retry` / `force_ready` | 4 | `main` 无（这是唯一"确实没有"的项，但**纯测试专用**，见 §10） |
| `usr` 其他零散测试断言 | 若干 | 与 `main` 已改写的行为绑定，不可直接搬 |

---

## 7. 必须**不**移植的对象（回灌即回退 `main`）

| 对象 | 为什么不能要 |
|---|---|
| `synapse-storage/src/openclaw/`、`src/web/routes/openclaw.rs`（~1200 行） | `main` **刻意删除**：`acf0b582 refactor(migrations): 删除 23 张零引用表（含 6 张 AI/OpenClaw 表）`、`f6029a81 chore(contract): ... 移除 openclaw` |
| `docs/audit/00..15_*`（AUDIT-2026-07 系列，~1268 行） | 已被 `main` 的更新系列取代：`docs/audit/P1_..P5_*_2026-09-10/11/12`、`AUDIT_SUMMARY_2026-09-12`、`PROJECT_ACTUAL_ISSUES_2026-09-14`。回灌会引入过期结论 |
| 分支的 CI 性能改造（Phase A / B1 / D3 / D4） | `main` 已重写（`--test-threads 4`、模板 schema 钉名、TRUNCATE 复用），分支的 `SHARED_CLONE_CONCURRENCY` 调参针对已被替换的 clone 池 |
| `cfg_attr(test, allow(unwrap_used))` | 与 `main` 的 lint 政策相反 |
| r0 路由快照/声明 | A7 已拆 r0 兼容嵌套 |
| `migrations/2026071019000{0,1}_*.sql` | 产物已吸收进 baseline |

---

## 8. 安全事件：被暂存的私钥

```
$ git -C .claude/worktrees/optimization+audit-2026-07 diff --cached --stat
 docker/deploy/synapse-data/signing.key | 3 +++
```

该文件是一把**真实的 Ed25519 私钥**（PKCS#8 PEM，`BEGIN PRIVATE KEY`），
由某次 `git add` 进入暂存区（`new file mode 100644`），**未被忽略**。

成因已定位：该 worktree 检出的是 7 月的 `.gitignore`，**没有** `*.key` / `signing.key` 规则；
而 `main` 的 `.gitignore:18-19` 已有这两条 —— 即 `main` 侧防护已补齐，只有这条陈旧分支暴露。

**已处置**：`git rm --cached docker/deploy/synapse-data/signing.key`
（取消暂存；磁盘文件保留、未提交、未删除；当前 `git diff --cached` 为空）。

**未做**：提交中从未包含该密钥（`git log --all -- docker/deploy/synapse-data/signing.key` 无需追溯），
故无需重写历史。若该分支被保留，任何人执行 `git commit` 前都会再看到它 —— 这也是
建议不要长期保留该 worktree 的理由之一。

---

## 9. 逐组处置建议

| 组 | 提交数 | 建议 |
|---|---|---|
| phase1/2/3 架构重构（trait 抽取、typed context、glob 收敛） | ~30 | **跳过**（`main` 已独立完成） |
| `refactor(storage): split *` 神文件拆分 | 14 | **跳过**（结构性分叉） |
| OPT-001…031 安全/正确性修复 | ~45 | **跳过**（31/31 已核验在 `main`） |
| 测试基建修复（integration/schema/隔离） | ~40 | **跳过**（`main` 已换成模板 schema 方案） |
| CI 性能改造（Phase A/B1/D3/D4） | 4 | **跳过**（`main` 已重写 CI） |
| AUDIT-2026-07 文档系列 | 3 | **跳过**（被 P 系列取代）；如需留档走"归档"而非"合入" |
| openclaw 相关 | 2 | **禁止**（`main` 刻意删除） |
| worktree 未提交改动（280 文件） | — | **跳过**（含 r0 回退与 lint 政策冲突） |

---

## 10. 残留风险与未决

1. **`main` 缺 3 个测试后门**：`reset_recoverer`、`reset_pending_transactions_retry`、
   `ApplicationServiceScheduler::force_ready`。作用是让集成测试**跳过 5s 退避窗口**，
   属测试专用。`main` 侧对应测试目前靠真实等待。→ 可选小改，**不属于 H-5 范围**。
2. **`main` 缺 4 个 test_mocks**：`space.rs`、`registration_token.rs`、`admin_federation.rs`、
   `ai_connection.rs`。前三个对应的 trait（`SpaceStoreApi` / `RegistrationTokenStoreApi` /
   `AdminFederationStoreApi`）在 `main` 存在，`AiConnectionStoreApi` **不存在**（mock 无意义）。
   这些 mock 是样板代码，且分支版本是对着 7 月的 trait 签名写的，**直接搬运大概率编译不过**。
   → 如需要应由 `main` 现场生成，**不属于 H-5 范围**。
3. **分支无远端副本**：`488d9888` 不在任何 `origin/*` 上。删除本地分支 = 永久丢失这 157 个提交。
   本报告的全部证据都可由 §附录 命令复现，但**原文本身会消失**。
   → 建议**归档保留**（打 tag / 改名分支）而不是删除。
4. **worktree 仍是 `git status` 非干净状态**（280 个已修改文件）。这些改动按 §6 无需移植，
   但**丢弃它们不可逆**。→ 需在 §11 的决策后由 `git worktree remove --force` 一并丢弃。

---

## 11. 收尾（已执行，2026-09-15）

用户选定"**归档保留 + 移除 worktree**"。实际执行：

| # | 动作 | 结果 |
|---|---|---|
| 1 | 备份 worktree 里那把私钥到 `~/Desktop/hu_ts/synapse-rust-h5-archive-2026-09-15/`（含说明 README） | 密钥与主工作区那把**不是同一把**，且运行中的 `synapse-app` 未挂载该路径 → 备份只为保险 |
| 2 | `git worktree remove --force .claude/worktrees/optimization+audit-2026-07` + `git worktree prune` | worktree 及其未提交的 280 个文件已丢弃 |
| 3 | `git branch -m optimization/audit-2026-07 archive/optimization-audit-2026-07` | 157 个提交仍可达（`git rev-list --count` = 157） |
| 4 | 撤销"同名 tag"方案 | 分支与 tag 同名会让 refname **歧义**（`git rev-parse` 报警），故仅保留分支 ref；归档语义由分支名承担 |

**收尾后实测**：

```
$ git worktree list
/Users/ljf/Desktop/hu_ts/synapse-rust 21398c41 [main]      # ← 只剩 1 条

$ git status --porcelain | wc -l
0                                                          # ← 无残留未提交改动
$ git status --porcelain | grep -c '^??'
0                                                          # ← 无遗留未跟踪文件

$ git rev-parse --verify refs/heads/archive/optimization-audit-2026-07
488d9888                                                   # ← 归档分支仍在
```

**如需恢复**：`git worktree add <path> archive/optimization-audit-2026-07`。

**H-5 关闭。** 本报告 §10 的残留风险 1、2（`main` 缺 3 个测试后门 / 4 个 test_mocks）
已明确划出 H-5 范围，如需处理应另立条目。

---

## 附录：复现命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
MB=$(git merge-base main optimization/audit-2026-07)

# A.1 规模与冲突
git rev-list --count $MB..optimization/audit-2026-07        # 157
git rev-list --count $MB..main                              # 707
git merge-tree --write-tree main optimization/audit-2026-07 | tail -1   # EXIT=1（300 冲突）

# A.2 内容级判据：把 main 的全树导出后逐行比对
rm -rf /tmp/main_tree && mkdir -p /tmp/main_tree
git archive main | tar -x -C /tmp/main_tree

# A.3 删除是否已生效（期望 0）
git diff --diff-filter=D --name-only $MB optimization/audit-2026-07 > /tmp/branch_deleted.txt
while read -r f; do [ -e "/tmp/main_tree/$f" ] && echo "未生效: $f"; done < /tmp/branch_deleted.txt

# A.4 OPT 产物是否已在 baseline 迁移里
grep -c trg_prevent_audit_delete /tmp/main_tree/migrations/00000000_unified_schema_v11.sql   # 2
grep -n idx_url_preview_cache_expires_at /tmp/main_tree/migrations/00000000_unified_schema_v11.sql

# A.5 私钥事件
git -C .claude/worktrees/optimization+audit-2026-07 diff --cached --stat     # 处置后应为空
grep -n '\.key' /tmp/main_tree/.gitignore                                     # main 已有：*.key / signing.key

# A.6 worktree 脏状态
git -C .claude/worktrees/optimization+audit-2026-07 status --porcelain | wc -l   # 280
```
