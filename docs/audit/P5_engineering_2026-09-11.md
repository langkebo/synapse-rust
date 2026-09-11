# P5 — 工程质量与防复发

> **审查日期**: 2026-09-11
> **基线**: `506e41a6`（迁移/部署三项修复后），工作树对本次审查干净
> **范围**: lint 门禁真实性、CI blocking 有效性、死代码、仓库治理、god-file
> **方法**: 先实测门禁"是否真的在验证东西"，再处理能安全闭环的项；不能安全闭环的如实记录边界

---

## 0. 结论摘要

| 状态 | 数量 | 项 |
|---|---|---|
| ✅ **已修复** | 4 | doc-test 空门禁 · `--tests` 构建被 clippy 拒绝 · `1necho` · 空目录 |
| 🟡 已量化并移交 | 3 | clippy cosmetic 警告 15 条 · `cargo doc` 3.5k 警告 · 157 处 `allow(dead_code)` |
| ⚪ 未测（如实标注） | 3 | god-file 漂移 · mock 漂移 · 构建产物治理 |

> **P5 的核心判断：仓库的"门禁数量"远大于"门禁效力"。**
> 15 个 workflow、多道 lint/测试步骤，但其中至少 2 道（doc-test、synapse-federation 的 `--tests`）
> 处于"看着绿、实际什么都没验证或本该失败"的状态。下面每一项都附实测证据。

---

## 1. ✅ 已修复

### 1.1 doc-test 门禁是空门禁，且曾放过一个真实编译错误

**缺陷**（`ci.yml` 原 `Run doc tests` 步骤）：
```yaml
- name: Run doc tests
  run: cargo test --doc --locked          # 只编译根 package
```
根 package `synapse-rust` 有 **0 个 doc test** ⇒ 该步骤永远输出 `running 0 tests` 并通过。

**危害已实证**：该门禁曾以全绿状态放过一个 rustdoc 专属编译错误 ——
本会话 P0 复核期间由 `--workspace` 首次暴露：

```
error[E0106]: missing lifetime specifiers
   --> src/federation/edu.rs:562:74
562 | fn validate_profile_update_content(edu: &Value, origin: &str) -> Option<(&str, Option<&str>, Option<&str>)>
```

**为何其它门禁都测不到**：返回值的生命周期从未被校验，
`cargo build` / `cargo check` / `cargo clippy` 均不报错，**只有 rustdoc 编译会报**。

**对照实验**：

| 命令 | 结果 |
|---|---|
| `cargo test --doc --locked`（原 CI 口径） | `EXIT=0`，**0 tests** |
| `cargo test --doc --locked --workspace`（修复后） | `EXIT=0`，真实编译各 crate 的 doc |

**修复**：改为 `--workspace`，并在 workflow 注释中写明捕获对象与**已知局限**。

**⚠️ 不得误认为这是 doc-test 覆盖率**：全 workspace 仅 4 个 doc test，且**全部 `#[ignore]`d**，
没有任何 doc 示例被真正执行。这是一道 **rustdoc 编译门禁**，不是执行覆盖。

### 1.2 `synapse-federation` 的 `--tests` 构建被 clippy 拒绝

**缺陷**（由 `cargo clippy --workspace --all-targets --all-features` 暴露）：
```
error: `panic` should not be present in production code
   --> synapse-federation/src/edu.rs:258
error: could not compile `synapse-federation` (lib test)
```
即**该 crate 的测试构建无法通过自身的 lint 配置**。

**CI 未捕获的两个叠加原因**：
1. `ci.yml` 的 `Run clippy` 只检 lib 范围（不含 `#[cfg(test)]` 块）
2. 唯一的测试范围 clippy step 只覆盖 `-p synapse-services`（workflow 注释自述为
   "避免整个 workspace test 编译时间爆炸"），`synapse-federation` 不在其中

**根因**：`synapse-federation/Cargo.toml` 设 `panic = "deny"`，而 lib.rs 的测试豁免为
`#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]` —— **不含 `panic`**。
且 `expect_used = "deny"` 同样无豁免 ⇒ 该 crate 的测试**不能用 panic!/expect/unwrap 中任何一种**。

**修复**：改为 `Result` 返回型测试（Rust 测试的标准零 panic 惯用法）：
```rust
fn test_edu_type_display_matches_from_str() -> Result<(), UnknownEduType> {
    ...
    let parsed = EduType::from_str(&s)?;   // ? 传播，失败信息不损失
    Ok(())
}
```

**证据**：

| 命令 | 修复前 | 修复后 |
|---|---|---|
| `cargo clippy -p synapse-federation --tests -- -D warnings` | 编译失败 | ✅ `EXIT=0` |
| `cargo clippy --workspace --all-targets --all-features` | ❌ `EXIT=101` | ✅ **`EXIT=0`**（0 编译错误） |
| `cargo nextest -p synapse-federation` | 182 passed | ✅ 182 passed |

### 1.3 仓库治理：stray 文件与空目录

| 项 | 处理 |
|---|---|
| 根目录 `1necho`（0 字节，**已被 git 跟踪**，疑似 `echo > 1necho` 手滑） | 已 `git rm` |
| `synapse-cache/src/cache/` 空目录（`lib.rs` 无 `mod cache` 引用） | 已删除（git 不跟踪空目录，仅工作区清理） |

> 说明：仓库唯一的另一处 0 字节跟踪文件是 `tests/.gitkeep`，属**有意保留**，未触碰。

---

## 2. 🟡 已量化，移交后续

### 2.1 clippy cosmetic 警告 15 条（test 构建）

workspace `--all-targets` 下的非阻塞警告：

| crate | 数量 | lint |
|---|---|---|
| `synapse-e2ee` (lib test) | 7 | `bool_assert_comparison`（`assert_eq!(x, true)`）等 |
| `synapse-storage` (lib test) | 4 | `field_reassign_with_default` 等 |
| `synapse-cache` (lib test) | 1 | — |

全部可用 `cargo clippy --fix --lib -p <crate> --tests` 自动修，**不影响编译**。
是否把它们纳入 CI（即扩大 clippy 测试范围到全 workspace）需要权衡 CI 时长，
**本次未擅自扩大门禁范围**。

### 2.2 `cargo doc` 约 3,500 条 unresolved intra-doc-link 警告

`cargo doc --no-deps --workspace` → `EXIT=0` 但 **3,487 条 warning**，主要是
`unresolved link to \`new\`` / `\`get_stats\`` 等简写 intra-doc link。

⇒ **不能**直接把 `cargo doc --workspace -- -D warnings` 作为门禁（会立刻爆红）。
若要治理，应先批量修正为完整路径（`[\`new\`]: Self::new` 之类），再启用门禁。

### 2.3 157 处 `#[allow(dead_code)]` / `#[allow(unused*)]`

规模本身是死代码债的输入。P1 阶段抽查未发现被豁免的**安全校验**函数，
但未逐一核查 ⇒ 保留为待办。

---

## 3. ⚪ 未测（如实标注，不作为结论）

以下项属 P5 范围但**本会话未实测**，不得引用为结论：

1. **god-file 漂移** —— ✅ **本轮已量化**（见 §3.2）。结论：标题行数**显著高估**了
   生产代码体量，因为大文件约一半是内联测试；最大的纯生产文件是
   `friend_room_service/mod.rs`（1,831 行）。**未执行拆分**（理由见 §3.2）。
2. **mock 漂移** —— 🟡 **本轮已抽样核查**（见 §3.3）：`get_mutual_rooms_between`
   的 mock 与 PostgreSQL 实现**逐条规则一致**；但发现 mock 对**负 limit** 的行为
   与真实实现**相反**（详见 §3.3）。
3. **构建产物治理**：`target/` 92G + `docker/deploy` 18G + `target_amd64` 4.0G
   + `target_arm64` 3.8G ≈ **118G**；未处理（属环境清理，且 `docker/deploy` 可能含部署数据，
   贸然清理有风险）。

---

## 3.1 ⚠️ 实测发现：并发 cargo 构建会互毁 `target/`（本仓库当前正发生）

本会话存在另一个 agent 在同一工作区持续提交（提交者 `langkebo`）。

**现象**：`cargo clippy --workspace --all-targets` 与 `cargo test --doc --workspace`
**刚验证通过后不久即失败**，错误形态与代码无关：

```
error: failed to build archive at .../target/debug/deps/libsignal_hook_registry-<hash>.rlib:
       failed to open object file: No such file or directory (os error 2)
error[E0463]: can't find crate for `http_body`
error: extern location for http does not exist: .../target/debug/deps/libhttp-<hash>.rmeta
```

**判定**：这是两个 cargo 进程同时读写同一 `target/` 目录造成的**产物损坏/缺失**，
不是 lint 或代码缺陷（同一命令在无并发时 `EXIT=0`）。

**影响**：任何在共享工作区并行运行的构建/测试/门禁，其**结论不可信** ——
既可能假红（如本例），也可能掩盖真实问题。这与 P0 记录的
"integration 结果对并发度高度敏感"是同一类问题在不同层面的表现。

**规避**：在存在并发写者的环境下验证时，使用隔离 target 目录：

```bash
CARGO_TARGET_DIR=/tmp/verify cargo clippy --workspace --all-targets --all-features --locked
CARGO_TARGET_DIR=/tmp/verify cargo test --doc --locked --workspace
```

> 建议：若不需要并行协作，应停掉第二个 agent；否则所有门禁结论都必须附
> "是否使用了隔离 target 目录 / 是否存在并发构建"。

### 3.2 god-file 量化（本轮实测）

**关键区分：标题行数包含内联测试段，生产代码体量远小于表面数字。**

| 文件 | 总行数 | 测试段起始 | 生产代码约 | 性质 |
|---|---|---|---|---|
| `src/web/api_doc/client_server.rs` | 5,056 | — | — | **自动生成的 OpenAPI 文档**，非维护对象 |
| `src/web/api_doc/admin.rs` | 2,504 | — | — | 同上（`api_doc/` 合计 ~3,400 行） |
| `synapse-services/src/sync_service/tests.rs` | 2,371 | — | — | **纯测试文件** |
| `synapse-storage/src/event/db_tests.rs` | 2,183 | — | — | **纯测试文件** |
| `synapse-storage/src/room/mod.rs` | 2,270 | 1,449 | **1,448** | 生产（含内联测试） |
| `synapse-storage/src/device/mod.rs` | 2,192 | 1,316 | **1,315** | 同上 |
| `synapse-storage/src/refresh_token/mod.rs` | 2,186 | 1,101 | **1,100** | 同上 |
| `synapse-storage/src/membership/mod.rs` | 2,105 | 1,068 | **1,067** | 同上 |
| `synapse-services/src/friend_room_service/mod.rs` | 1,833 | 1,832 | **1,831** | 生产（几乎无内联测试） |

**判定**：
- 表面最"god"的两个 5K/2.5K 文件是**生成产物**（`api_doc/`），不应计入债务
- 其次两个 2.3K/2.2K 文件是**纯测试文件**，拆分收益低
- **真正值得关注的是 4 个 1,000–1,450 行的生产文件**
  （`room`/`device`/`refresh_token`/`membership` 的 `mod.rs`）
  以及 **1,831 行的 `friend_room_service/mod.rs`**（几乎无内联测试）

**为什么本轮不执行拆分**：
1. 本仓已有拆分先例（`room/service.rs` + `room/storage.rs`、`synapse-cache` 从 2,500 行
   拆为 `local`/`remote`/`manager`、`src/web/routes/handlers/`），因此**方向可行**；
2. 但 1,000–1,800 行属于**可控区间**，不是 5,000 行的维护灾难；
3. 大规模文件重排会与工作区中并发的提交产生冲突，并可能影响
   `scripts/shell_routes_allowlist.txt` 这类**基于行号**的豁免（本仓已知脆弱点）；
4. 拆分属**独立重构决策**，应有明确收益目标（如降低合并冲突率、提升可导航性），
   而非为了满足行数阈值。

**建议**：若确要拆分，优先 `friend_room_service/mod.rs`（1,831 行且无测试分担），
按领域拆为 `friends`/`requests`/`groups` 子模块，并在同一变更中更新 route manifest 与快照。

### 3.3 mock 漂移抽样核查（本轮）

`test_mocks` 共 **33 个模块、约 11,000 行**（最大 `event.rs` 1,246、
`tests.rs` 1,186、`member.rs` 700）。无法逐一核查，故按"**业务规则最易漂移**"
（排序/分页/过滤/边界）抽样。

**抽检对象**：`get_mutual_rooms_between`（近期新增特性，mock 与真实实现并存）。

| 规则 | PostgreSQL 实现 | Mock 实现 | 一致 |
|---|---|---|---|
| 仅 `membership = 'join'` | `a.membership='join' AND b.membership='join'` | 两侧 filter `== "join"` | ✅ |
| 求交集 | 自连接 | `HashSet::intersection` | ✅ |
| 排序 | `ORDER BY a.room_id` | `rooms.sort()` | ✅ |
| 游标过滤 | `a.room_id > $3` | `retain(|r| r.as_str() > after)` | ✅ |
| `has_more` 探测 | `LIMIT limit+1` 后比较 | `len() > limit` | ✅ |
| `next_batch_token` | `rooms.last().cloned()` | `result.last().cloned()` | ✅ |

⇒ **该接口的 mock 忠实反映了真实行为。**

**🔴 但发现一处方向相反的漂移**：对**负 `limit`**：

| | 行为 |
|---|---|
| PostgreSQL | `ERROR: LIMIT must not be negative` ⇒ 经映射成为 **HTTP 500** |
| Mock | `take(limit as usize)` ⇒ `-1 as usize` = `usize::MAX` ⇒ **返回全部数据** |

⇒ **任何依赖 mock 的测试都无法发现负 limit 的 500 问题**（mock 会"成功地"返回全部）。
这正是 mock 漂移导致 false-green 的典型形态。

**该漂移已通过另一路径暴露并修复**（见 §3.4）：源码级守卫测试而非 mock 行为测试。

#### 3.3.1 ✅ 已修复：mock 内部不一致（commit `a48cf1ad`）

同一 mock 文件内两个函数做法不一致：

| 函数 | limit 处理 |
|---|---|
| `get_room_members_paginated` | ❌ `truncate(limit as usize)` —— 无钳制 |
| `get_room_members_paginated_with_profiles` | ✅ `limit.clamp(1, 1000)` |

已统一为 `.clamp(1, 1000)`。**可达性核实**（避免夸大）：唯一生产调用链是
`admin/room/mod.rs:455`（**已** `.clamp(MIN, MAX)`）→ 服务层（不钳制）→ storage，
故**当前无活跃 bug**；修复价值在于让 mock 不再比生产更宽松、从而不再掩盖同类缺陷。

**新增回归测试** `paginated_mock_clamps_non_positive_and_huge_limits`
（limit = -1 / 0 / `i64::MAX`），并做了 **RED 验证**：临时移除钳制后该测试
如期 FAILED（`negative limit must clamp to 1, not return every member`），
还原后通过 —— 证明该测试确实能捕获原缺陷，而非"写了必过的断言"。

### 3.4 顺带发现并修复：分页 `limit` 缺下界 ⇒ 客户端输入触发 500

**发现路径**：核查 mock 时注意到 `take(limit as usize)`，反查 handler 的 `limit` 解析，
发现 `.min(1000)` **缺下界**（commit `0e0bf0e9`）。

**缺陷**：`?limit=-1` → 负 LIMIT → DB 报错 → `database_with_context` → **HTTP 500**；
`?limit=0` → 返回**空页却带 `next_batch_token`** → 诱导客户端无限翻页。

**修复 3 处**（与已被认可的 `metadata.rs` `.clamp(1, 100)` 一致）：

| 位置 | 形态 |
|---|---|
| `room/members.rs:306` | `i64` + SQL 分页 |
| `room/management/query.rs:147` | `i64` + SQL 分页 |
| `room/members.rs:356` | `usize` + 内存 slice 分页 |

**新增源码级守卫** `tests/unit/test_pagination_limit_clamp_tests.rs`：
扫描所有"解析 limit 查询参数"的行，断言必须含 `.clamp(`。

> 该守卫在开发过程中**发现了我人工 grep 漏掉的第 3 处站点**（`members.rs:356`），
> 并在初次运行时如实 FAILED 列出全部违规行 —— 正是"守卫优于人工检查"的例证。

### 3.5 ✅ 已修复：分页 `next_batch` 取 (limit+1) 项 + 严格 `<` 谓词 ⇒ 跳过一行

**发现路径**：为 `audit_event` mock 写 `total` 语义回归测试时，断言"第二页应返回剩余 1 行"
却实测得到 **0 行**。追查发现该行为**不是 mock 的问题，而是生产代码的分页语义**。

**缺陷机制**：

1. 查询 `LIMIT limit + 1`（为探测 `has_more` 多取一行）
2. `next_batch` 取 **`rows.get(limit)`** —— 即第 (limit+1) 项，**该项尚未被返回**
3. 续页谓词为**严格** `(created_ts, event_id) < cursor`
4. ⇒ 第 (limit+1) 项**既不在此页返回，也不在下一页返回** —— **被永久跳过**

**用可复现的数据说明**（3 行、`limit = 2`）：

| 页 | 返回 | cursor 指向 |
|---|---|---|
| 第 1 页 | 第 1、2 行 | 第 **3** 行 |
| 第 2 页 | **0 行** | — |

即客户端**永远看不到第 3 行**。

**仓库内两种约定并存**：

| 约定 | `next_batch` 取值 | 判定 |
|---|---|---|
| **A** | 最后一个**已返回**项 | ✅ 无跳过 |
| **B** | `rows.get(limit)`（第 limit+1 项） | 🔴 跳过一行 |

| 文件 | 约定 |
|---|---|
| `membership/mod.rs:1063` · `admin_media.rs:180` · `server_notification/repository.rs:130,705` · `room/admin.rs:691` · `background_update.rs:331` · `module.rs:564`（均为 `.last()` 或等价） | **A** ✅ 无需改动 |
| `audit.rs:223` · `room/mod.rs:427` · `registration_token/repository.rs:263`（`get(limit)`） | **B** 🔴 **已修复** |

> ⚠️ **范围更正**：本节初稿按 `== limit` / `> limit` 的**条件写法**粗分类，得出"6+ 端点"，
> 偏大。按**实际取值方式**（是否用 `get(limit)`）精确核对后，真正有缺陷的只有 **3 处**。
> 教训：分类应基于行为而非表面条件写法。

**外部佐证**：上游 Synapse 有专门修复 PR
[#13840 "Fix skipping items when paginating /relations forward"](https://github.com/matrix-org/synapse/pull/13840/files/8828fa7c03c1ffe4d6186262a40dd7b901cad29d..8ce01fbfcd976c541ba3f61e2a6eab8e55b136f2#1)
—— 标题直指同一类缺陷（分页跳过条目）。

**✅ 已修复（commit `862726e6`）**：3 处 `get(limit)` → `get(limit.saturating_sub(1))`，
把 cursor 锚定到**最后一个已返回行**。配合既有的严格 `<` 谓词，下一页恰好从上一页末行
之后继续 —— 无跳过、无重复。

**同步**：`audit_event` mock 一并改为锚定末行，使 mock 与生产语义一致。

**回归测试**（2 个）：
- `list_events_total_is_independent_of_cursor`：第二页断言 `len == 1`（修复前为 0）
- `paginating_visits_every_row_exactly_once`：**完整遍历不变量** —— 5 行 + limit=2
  逐页走完，断言访问集合 == 插入集合

**RED 验证**（证据确凿）：临时恢复缺陷 cursor 后，不变量测试如期 FAILED：
```
left:  ["$w0", "$w1", "$w3", "$w4"]        ← $w2 被跳过
right: ["$w0", "$w1", "$w2", "$w3", "$w4"]
```
还原后通过 ⇒ 测试确实能捕获该缺陷，而非"必过断言"。

**已补齐（commit `bbe26f60`）**：为两个未覆盖端点补了回归测试，并在此过程中发现
**一处更严重的 mock 缺口** —— `test_mocks/room.rs` 的 `get_all_rooms_with_members`
**完全不支持分页**：

```rust
let _ = (from, order_by);      // cursor 与排序参数被丢弃
filtered.truncate(limit as usize);
Ok((filtered, None))           // next_batch 恒为 None
```

⇒ 传 cursor 会**返回与第 1 页相同的房间**，且永远没有 `next_batch` ——
任何依赖该 mock 的分页测试都无法成立。已按真实实现重写（三种排序的 keyset 谓词与
tie-break、`LIMIT limit+1` 探测、cursor 锚定末行），并补 2 个测试：

| 测试 | 保护对象 |
|---|---|
| `room_search_pagination_visits_every_room_exactly_once` | 5 房间 + limit=2 遍历访问集合 == 创建集合 |
| `room_search_next_batch_presence_follows_limit` | limit < 集合 ⇒ 有 token；limit >= 集合 ⇒ 无 |

**RED 验证**：旧 mock 下"恰好一次"测试得到 `left: ["!r1:t", "!r2:t"]`（仅 2 个，
分页失效），修复后 2 passed。

同时修复 `test_mocks/registration_token.rs` 的同款 `get(limit)` cursor 缺陷。

### 3.6 mock 漂移核查阶段结论（4 轮）

按"业务规则最易漂移"抽样核查，累计发现 **5 处** mock 与生产的实质差异：

| # | 位置 | 差异性质 | 处理 |
|---|---|---|---|
| 1 | `member.rs` `get_room_members_paginated` | 负 `limit` 未钳制 ⇒ mock 返回全部、生产报错 | ✅ 已修 + RED 验证 |
| 2 | `audit_event.rs` `list_events` 的 `total` | 在 cursor 过滤后计算 ⇒ 随翻页递减（生产为恒定总数） | ✅ 已修 + 测试 |
| 3 | `audit_event.rs` 分页 cursor | 取 `get(limit)`（第 limit+1 项） | ✅ 已修 + 遍历不变量 + RED |
| 4 | `room.rs` `get_all_rooms_with_members` | **完全不支持分页**（丢弃 `from`/`order_by`、`next_batch` 恒 `None`） | ✅ 已重写 + 2 测试 + RED |
| 5 | `registration_token.rs` 分页 cursor | 同 #3 | ✅ 已修 |
| 6 | `device_list.rs` `get_device_list_changed_users` | **忽略 `from`/`to`/`requester`，返回所有用户**（生产为 stream 窗口 + 排除请求者 + 排序 + `LIMIT 100`） | ⚠️ **仅加警告，未修** |

#### 关于 #6（未修）的说明

该 mock 被 **7 个文件**使用（含 `sync_service/tests.rs`）。**未修的原因**：
`InMemoryDeviceListStore` 只有**一个全局 `stream_id` 计数器**，**没有每设备的 stream
位置** ⇒ 要正确实现窗口过滤必须改数据模型（在设备写入时记录 stream 位置）。
这是刻意为之的较大任务，不应在核查中顺手改。

**本轮所做**：把原来含糊的 `// Simplified: return all users that have devices`
升级为**显式警告块**，写明与真实实现的逐项差异、因此**不可测的四个维度**
（stream 窗口语义 / 请求者排除 / 排序 / `LIMIT 100` 上限）以及未修原因 ——
使后续读者不会把这个 stub 当作忠实 mock 使用。

#### 方法论小结

有效的扫描形态（按命中率排序）：
1. **丢弃参数**（`let _ = (from, order_by)`）—— 命中 #4，危害最大
2. **下划线前缀的功能性参数**（`_from`/`_limit`）—— 命中 #6
3. **无保护的转换**（`limit as usize`）—— 命中 #1、#5
4. **在过滤之后才计算的聚合量** —— 命中 #2

**未完成**：其余约 27 个 mock 模块的**过滤 / 排序 / 默认值**维度尚未系统核查。

---

## 4. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 1) doc-test 门禁：根级 vs workspace 级的对照
cargo test --doc --locked              # 0 tests —— 原 CI 口径，什么也不验证
cargo test --doc --locked --workspace  # 真实编译各 crate doc

# 2) clippy 测试范围盲区
cargo clippy --all-features --locked -- -D warnings                      # CI 口径，绿
cargo clippy --workspace --all-targets --all-features --locked           # 暴露 test 构建问题
cargo clippy -p synapse-federation --all-features --tests --locked -- -D warnings  # 曾编译失败

# 3) 治理项
git ls-files 1necho                    # 修复前有输出，现在为空
find synapse-cache/src -type d -empty  # 修复前有 synapse-cache/src/cache

# 4) 量化残留
cargo doc --no-deps --workspace 2>&1 | grep -c '^warning'    # ~3487
grep -rn '#\[allow(dead_code)\]\|#\[allow(unused' --include='*.rs' src/ synapse-*/src/ | wc -l  # 157
```

---

## 5. 移交后续阶段

| 项 | 目标 |
|---|---|
| 是否将 clippy 测试范围扩到全 workspace（需权衡 CI 时长） | **P5 后续 / 用户决策** |
| `cargo doc` 3.5k intra-doc-link 警告治理后再启用门禁 | **P5 后续** |
| 157 处 `allow(dead_code)` 抽查是否存在未接线逻辑 | **P5 后续** |
| god-file 拆分评估、mock 漂移核查 | **P5 后续** |
| 118G 构建产物治理 | **P5 后续（环境清理）** |
