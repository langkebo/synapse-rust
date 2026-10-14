# 上游 Synapse `changelog.d` 对齐优化方案（2026-10-13）

> **数据源**：`element-hq/synapse` @ `develop`，`changelog.d/`（towncrier 未发布条目）。采集时间 2026-10-13。  
> **本仓基线**：`main` @ `ed09854e5`（工作区干净）。  
> **性质**：本文件是**方案**（audit + plan），不改动任何代码。凡"待核实"项均给出一行可复现的核实命令。

---

## 0. TL;DR

- 上游 `changelog.d` 共 **24 条**未发布条目，可归为 6 类：规格宣告、MSC 特性、Sticky Events 修复、联邦/资料、安全隐私、性能正确性。
- 本仓**已经对齐**的（无需行动）：MSC4222 双拼写、MSC2965 `auth_metadata`、20321 错误不泄露内部细节、20006 媒体孤儿文件（结构上不适用）。
- 本仓**确有差距**、建议排期的：
  1. **`/versions` 只宣告到 `v1.14`，上游已到 `v1.16` + `v1.17`**（最直接、最可验证的差距）。
  2. **MSC4210**：默认推送规则仍带三条 legacy 提及规则，上游与规格 v1.17 默认已移除。
  3. **MSC4452**：`preview_url` 只声明了不稳定别名，缺稳定能力名 `m.preview_url`。
  4. **MSC4354 Sticky Events**：本地有实现，但上游这轮有 **4 条修复**（历史可见性豁免覆盖、redact 与 sticky 的交互、到期口径）本地大概率未收口。
  5. **appservice `url: null`**：本地注册模型把 `url` 定为**必填** `String`，上游允许 `null`（bridge 双傀儡注册）⇒ 上游合法的注册文件在本仓会解析失败。
  6. **MSC4140**：本地已有发送端 `delay` 提示 + 管理端点；上游新增**专用调度端点**并改了取消语义，需对照 PR 确认是否等价。
- 明确**不适用**：MSC4426/MSC4259（本仓无 user status）、SQLite→PG 序列（本仓 PG-only）。

---

## 1. 取证方法与可复现命令

上游条目是用 GitHub API 列目录 + 并行抓原始文件得到的；本仓判定全部来自对源码的实测检索（ripgrep），**不采信模块名或注释的自述**。

```bash
# 1) 列出上游未发布条目
gh api "repos/element-hq/synapse/contents/changelog.d?ref=develop" \
  --jq '.[] | select(.type=="file") | .name'

# 2) 逐条取内容（<name> 为上一行输出）
curl -sSL "https://raw.githubusercontent.com/element-hq/synapse/develop/changelog.d/<name>"
```

本仓核查一律走 `Grep`（ripgrep）；**不要**用裸 `grep` 加 `\|`——  
本机 shell 的 `grep` 被 toybox 替换，BRE 扩展 `\|` 会**静默返回空**，看起来像"确实没有"。

---

## 2. 上游未发布条目全量清单（24 条）

| 编号    | 类型      | 内容摘要                                                                                                 |
| ----- | ------- | ---------------------------------------------------------------------------------------------------- |
| 19354 | feature | MSC4140：新增**调度延迟事件的专用端点**                                                                            |
| 19495 | removal | MSC4140：不再因"另一用户发来更新的状态事件"而取消本用户的延迟状态事件                                                              |
| 20006 | bugfix  | 超过 `media_upload_limits` 的媒体上传会在磁盘留下无 DB 记录的孤儿文件                                                     |
| 20081 | bugfix  | 增量 `/sync` 中，当离开者是该房最后一个本地用户时 `state_after` 缺 leave 事件                                               |
| 20143 | removal | 移除不稳定的 MSC2965 `.well-known` 端点                                                                      |
| 20170 | feature | **稳定化 MSC4222**：`/sync` 支持稳定 `use_state_after` 与 `state_after`（Matrix v1.16），移除 `msc4222_enabled` 开关 |
| 20202 | bugfix  | 被服务端配置禁用的默认推送规则，经客户端 API 修改时应 404 而非成功                                                               |
| 20220 | removal | 默认移除遗留"提及"推送规则（MSC4210）；`msc4210_enabled` 默认改为 `true`                                                |
| 20243 | bugfix  | `request_token_inhibit_3pid_errors` 默认改为 `true`，防 `/requestToken` 泄露邮箱是否已注册                          |
| 20273 | bugfix  | 不再为 `url: null` 的 appservice（双傀儡注册）排队事件/临时数据                                                         |
| 20294 | misc    | 给 Synapse 的 Rust 侧加 Clock 抽象，并在 rendezvous 处理器中使用                                                    |
| 20295 | bugfix  | MSC4354 Sticky Events 的历史可见性豁免**覆盖全部 client-server 端点**                                              |
| 20298 | feature | 新增 `m.preview_url` 能力（稳定化 MSC4452）                                                                   |
| 20302 | bugfix  | SQLite→PostgreSQL 迁移时初始化 `un_partial_stated_room_stream_sequence`                                    |
| 20312 | misc    | 事件内部元数据结构加入 `received_ts`                                                                            |
| 20315 | misc    | 推送规则评估跳过 **sender 自身**的规则，提速                                                                         |
| 20321 | bugfix  | 修复 Pydantic 校验错误信息向客户端泄露内部细节                                                                         |
| 20322 | bugfix  | 同一持久化批次内被 redact 的 Sticky Event 不应 sticky                                                            |
| 20323 | bugfix  | 持久化**之后**被 redact 的 Sticky Event 需移除 sticky 性                                                        |
| 20324 | bugfix  | 修复 Sticky Event 到期时间"上报值与服务端追踪值不一致"                                                                  |
| 20328 | bugfix  | 远端资料查询时过滤 MSC4426 User Status 字段（对不支持 MSC4259 的客户端会变陈旧）                                              |
| 20331 | doc     | 文档化 `/auth_metadata` 端点可由 worker 处理                                                                  |
| 20333 | feature | 宣告支持 Matrix **v1.16**                                                                                |
| 20334 | feature | 宣告支持 Matrix **v1.17**                                                                                |

---

## 3. 逐条对照（本仓实测）

### 3.1 结论汇总表

| 上游条目                          | 本仓现状（实测位置）                                                                                                                                                  | 判定         | 行动         |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ---------- |
| 20333 / 20334                 | `/versions` 宣告 `v1.1`…`v1.14`（`synapse-services/src/capability_governance.rs:68-87`）                                                                        | **差距**     | ✅ 批次 A     |
| 20170                         | 已支持稳定 + 不稳定双拼写（`synapse-web/src/routes/handlers/sync.rs:237-241`）                                                                                           | 基本对齐       | 仅补测试       |
| 20081                         | 已专项验证（`state_events_since_batch_keeps_the_last_local_users_leave`）                                                                                          | **本仓不受影响** | ✅ 批次 A     |
| 20220                         | 默认规则仍含 **3 条 legacy** 提及规则（`synapse-common/src/push_rules.rs`）                                                                                              | **差距**     | ✅ 批次 B（B1） |
| 20298                         | 只声明不稳定别名 `io.element.msc4452.preview_url`，缺稳定名（`synapse-services/src/capability_governance.rs`）                                                             | **差距**     | ✅ 批次 B（B2） |
| 20295 / 20322 / 20323 / 20324 | 有 MSC4354 实现（`synapse-web/src/routes/room.rs:82-95,169`），4 条修复未见对应收口                                                                                        | **疑似差距**   | 批次 C       |
| 19354 / 19495                 | 发送端 `org.matrix.msc4140.delay`（`synapse-web/src/routes/handlers/room/mod.rs:141`）+ 管理端点（`synapse-web/src/routes/delayed_events.rs:97-98`）+ storage + 联邦 EDU | 待核实        | 批次 C       |
| 20273                         | appservice 注册模型 `url: String` **必填**（`synapse-services/src/application_service/models.rs:41-42`）                                                            | **兼容性差距**  | 批次 B       |
| 20243                         | 未找到该配置项或其等价防枚举逻辑                                                                                                                                            | 待核实        | 批次 B       |
| 20006                         | 全局 body 上限由路由层前置拦截（`src/server/router.rs:37-39`）⇒ 超限在落盘前 400/413                                                                                            | 不适用        | 无          |
| 20321                         | `ApiError::message()` 对 `Internal` 返回泛化文案，细节只进日志（`synapse-common/src/error.rs:888-900`）                                                                     | 已对齐        | 无          |
| 20202                         | 本仓无"配置禁用 server-default 推送规则"的开关                                                                                                                            | 不适用        | 无          |
| 20143                         | `.well-known/matrix/client` 仅回 `m.homeserver` / `m.tile_server`（`synapse-web/src/routes/handlers/versions.rs:142-154`）                                      | 已对齐        | 无          |
| 20302                         | 本仓 PG-only，无 SQLite 迁移路径                                                                                                                                    | 不适用        | 无          |
| 20312                         | 本仓无 `EventInternalMetadata` 结构                                                                                                                              | 架构差异       | 无          |
| 20315                         | 未定位到"按 sender 跳过规则"的评估路径                                                                                                                                    | 待核实        | 低优先        |
| 20328                         | 本仓无 MSC4426 User Status 功能                                                                                                                                  | 不适用        | 无          |
| 20294                         | Rust 侧无统一 Clock 抽象                                                                                                                                          | 可选改进       | 低优先        |
| 20331                         | 有 worker 门控；`/auth_metadata` 是否可由 worker 服务未标注                                                                                                              | 低优先        | 低优先        |

### 3.2 重点条目明细

#### 3.2.1 规格宣告落后（20333 / 20334）—— 最高性价比

本仓 `CLIENT_API_VERSION_SUPPORT` 的注释写着"只宣告 v1 家族"，但列表**止于 `v1.14`**，且 `v1.14` 那条还带"仅 1 个niche 缺口"的 2026-06-28 遗留说明。上游已并列宣告 **v1.16 与 v1.17**。

注意这**不是**"改个常量"那么简单：宣告一个版本等于承诺该版本引入的端点/字段可用。因此该项与 §2 中的 MSC4222 稳定化（v1.16）耦合，应同批处理，并配一条"宣告版本 ⇔ 端点存在"的守卫（见批次 A）。

#### 3.2.2 MSC4210 提及规则（20220）

> ⚠️ **本节初稿写错了清单**（曾把 4 条"提及类"规则一并列出）。已按 MSC4210 原文与上游实现  
> （`element-hq/synapse` 的 `rust/src/push/mod.rs:635-641`）订正为 **3 条**。

MSC4210 移除的是**纯文本匹配**类的三条规则：

- `.m.rule.contains_display_name`
- `.m.rule.contains_user_name`
- `.m.rule.roomnotif`

而 `.m.rule.is_user_mention` / `.m.rule.is_room_mention` 是**有意提及**（`m.mentions`）规则，  
**必须保留** —— 把它们一起删掉会静默破坏 `m.mentions`。

上游在 `msc4210_enabled` 为真时**隐藏**（而非删除）这三条；本仓初稿给出的四条清单与  
"连有意提及一起删"的正确做法之间差之毫厘——**MSC 原文才是判据**。

这是一条会直接改变客户端通知行为的语义变更（MSC 自述"旧客户端将收不到提及通知"）⇒  
引入 `msc4210_enabled` 开关，默认 `true`（对齐上游与规格 v1.17），并保留回退余地。

#### 3.2.3 MSC4452 `preview_url` 稳定化（20298）

上游 `synapse/rest/client/capabilities.py` 的权威写法：

- **`m.preview_url`**：**无条件**声明，`enabled = config.media.url_preview_enabled`；
- `io.element.msc4452.preview_url`：仅在 `experimental.msc4452_enabled` 时声明（注释原文  
  "Keep around while clients migrate away from MSC4452"），`enabled` 同上。

本仓此前**只声明了不稳定别名**，且默认关闭（`experimental.msc4452_enabled: false`）⇒  
只认稳定名的客户端在本仓**什么都发现不了**。

**核实中发现的连带事实（决定了实现只能怎么做）**：

- 承载 `url_preview_enabled` 的 `MediaStoreConfig` 在 `synapse-common/src/config/mod.rs`  
  里是**整段被注释掉的未实现 stub**（"官方 Synapse 配置模块（未实现）"）；
- 真正生效的 `config.url_preview`（`config/voip.rs` 的 `UrlPreviewConfig`）**只有  
  `ip_range_blacklist`，没有启用开关**。

⇒ 本仓**没有** media 级开关可用，因此稳定名只能报端点实际执行的那个开关  
（`experimental.msc4452_enabled`），否则"能力说 enabled、端点却 403"会自相矛盾。

#### 3.2.4 MSC4354 Sticky Events 的四条修复

本地实现挂在 `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events` 下（不稳定前缀，选择本身正确）。上游这轮四条修复都指向"sticky 与 redaction / 到期 / 历史可见性"的交互边界，属于**实现细节型缺陷**：本地有实现不等于有这些收口，需逐条构造回归。

#### 3.2.5 appservice `url: null`（20273）

`AppServiceConfigFile` 的 `url` 是**必填** `String`（无 `#[serde(default)]`）。上游允许 `url: null`（bridge 双傀儡注册的常见形态）。后果：**上游合法的 appservice 注册文件在本仓会直接反序列化失败**，服务起不来。上游修的是"别为它排队事件"，本仓要先能**接受**这种注册，顺带把"不排队"一并做对。

---

## 4. 优化方案

### 批次 A —— 规格版本对齐（P1，低风险高价值）

1. **A1**：把 `CLIENT_API_VERSION_SUPPORT` 补到 `v1.16`、`v1.17`；**逐条**核对两个版本相对 `v1.14` 的新增端点/字段，能实现的补实现，不能实现的在注释里写明缺口（沿用现有"v1.14 注释"的写法，但要求可回源）。
2. **A2**：新增守卫 `declared_versions_have_evidence`：对 `CLIENT_API_VERSION_SUPPORT` 里每个 `vX.Y`，断言其在文档/测试中有对应证据行；列表**不得**只增不改注释。
3. **A3**：为 20081 补一条增量 `/sync` 回归测试：构造"离开者是房内最后一个本地用户"，断言 `state_after` 含 leave 事件。先跑，看是否已通过——**通过即证明本地无此缺陷**，失败则按上游修法修。

- 验收：`/versions` 含 `v1.14`/`v1.15`/`v1.16`/`v1.17`；新增守卫在"删掉某版本证据"时变红（变异自证）。

### 批次 B —— 行为语义对齐（P1）

1. **B1（MSC4210）**：新增配置 `experimental_features.msc4210_enabled`，默认 `true`；为 `true` 时不下发 **3 条 legacy** 提及规则（`.m.rule.contains_display_name` / `.m.rule.contains_user_name` / `.m.rule.roomnotif`），**保留** `is_user_mention` / `is_room_mention`。默认值变更需在 CHANGELOG 与 `docs/` 明写。
2. **B2（MSC4452）**：**无条件**声明稳定能力名 `m.preview_url`（保留不稳定别名）；由于本仓无 media 级开关，两者都取 `experimental.msc4452_enabled`，与端点 403 门控保持一致。
3. **B3（appservice `url: null`）**：`url` 改为 `Option<String>`；`None` 时**跳过**事件/临时数据排队（对齐 20273），并在启动日志标注"该 appservice 不接收推送"。
4. **B4（20243）**：核实本地 `/requestToken` 是否可枚举账号；若是，加 `request_token_inhibit_3pid_errors`（默认 `true`）。

- 验收：每条配一个"行为可观察"的测试（不是断言常量）；B1/B3 需要变异自证。

### 批次 C —— 深度收口（P2）

1. **C1（MSC4354 四条）**：逐条写回归（redact 同批次 / redact 后 / 到期口径 / 历史可见性豁免覆盖全部 C-S 端点），失败的按上游修法修。
2. **C2（MSC4140）**：对照上游 19354 PR 确认"专用调度端点"与本地"发送体 delay 提示"是否等价；核对 19495 的取消语义（本地只有显式取消 ⇒ 需证明不存在隐式取消路径）。
3. **C3（20315）**：定位推送评估路径，确认是否已跳过 sender 规则；未跳过则补。
4. **C4（可选）**：Rust 侧 Clock 抽象（20294）——提升 rendezvous 的可测性，非阻塞。

### 批次 D —— 门禁与文档（与 A/B/C 同步）

1. 每批完成后更新 `docs/audit/INDEX.md`、`CHANGELOG.md`，并复跑全套 preflight（见 §5）。

---

## 5. 验证与门禁（每批完成后必跑）

```bash
export PATH="/Users/ljf/.workbuddy/binaries/python/versions/3.13.12/bin:/usr/bin:/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
cargo nextest run --test unit --features test-utils
bash scripts/quality/preflight.sh
```

⚠️ 改**推送规则默认集**或**版本宣告列表**都会波及快照/契约类测试；改路由字面量时须按项目既有顺序**连带重生成** 3 处派生产物（`docs/synapse-rust-vs-synapse-comparison.md` 的注册路由计数、`docs/openapi/route-table.json`、集成快照 `route_ledger_*.snapshot`），否则会制造"既有红"。

---

## 6. 待核实项（附可复现命令）

| #  | 问题                                  | 核实命令 / 方法                                                                     |
| -- | ----------------------------------- | ----------------------------------------------------------------------------- |
| V1 | 上游 v1.15/v1.16/v1.17 相对 v1.14 新增了什么 | 读 `matrix-spec` 对应版本的 changelog；或对比上游 `synapse/rest/client/versions.py`       |
| V2 | MSC4452 稳定化是否改默认开关                  | 读上游 PR #20298 与其 `experimental` 配置 diff                                       |
| V3 | MSC4140 是否需要"专用端点"                  | 读上游 PR #19354 的 REST 路由 diff，比对本地 `delayed_events.rs`                         |
| V4 | 本地 `/requestToken` 是否泄露账号存在性        | 对已注册/未注册邮箱各发一次 `/account/3pid/email/requestToken`，比较响应                        |
| V5 | 本地推送评估是否跳过 sender                   | `rg -n "sender" synapse-services/src/push* synapse-services/src/sync_service` |
| V6 | 20315 的提速是否适用本仓                     | 同 V5                                                                          |
| V7 | `/auth_metadata` 能否由 worker 服务      | `synapse-web/src/routes/worker.rs` 的门控清单是否含该路径                                |

---

## 7. 说明

- 本文件只做**判定与排期**，不含代码改动。上游条目对应的是 `develop`（未发布），存在**在发布前再次变更**的可能——实施前应重新拉一次 `changelog.d` 与 `CHANGELOG.md`。
- 上游是 Python 实现，本仓是 Rust 重写：**行为对齐**优先于**实现形状对齐**。凡"No"结论（如 20006、20321、20302）都给出了本仓的**结构性原因**，而不是"没找到"。

---

## 8. 批次 A 实施记录（2026-10-13）

**状态：A1 / A2 / A3 已实施。**

### A1 —— 版本宣告延到 v1.17

`synapse-services/src/capability_governance.rs`：

- `CLIENT_API_VERSION_SUPPORT` 由 `v1.14` 延到 **`v1.15` / `v1.16` / `v1.17`**（与上游 `rust/src/handlers/versions.rs` 的宣告面一致；上游另含 `r0.0.1`…`r0.6.1`，本仓**有意**不含，理由已在常量注释中写明）。
- 新增 `CLIENT_API_VERSION_EVIDENCE`：**逐版本证据表**。每行必须引用**自己的** `content/changelog/<version>.md`，并对本仓未实现的条目显式标注 `GAP:`（"声明即在明处"，不靠客户端运行时发现）。
  - ⚠️ 该表是 `#[cfg(test)]`：不加会触发 `dead_code`，在 `-D warnings` 下**变红**。

### A2 —— `declared_versions_have_evidence` 守卫

同文件新增纯谓词 `evidence_violations(support, evidence)` + 4 个测试：

| 测试                                               | 作用                                       |
| ------------------------------------------------ | ---------------------------------------- |
| `declared_versions_have_evidence`                | 两表 1:1（同序、同长），证据非空/非占位，且必须引用自身 changelog |
| `the_evidence_checker_rejects_a_copy_pasted_row` | 红证明：复制相邻行 ⇒ 判红                           |
| `the_evidence_checker_rejects_a_missing_row`     | 红证明：漏写一行 ⇒ 判红                            |
| `the_evidence_checker_rejects_a_placeholder`     | 红证明：证据里留 `TODO/FIXME/XXX` ⇒ 判红           |

"引用**自身** changelog"这一条是关键：它让"复制上一行改个版本号"这种最常见的敷衍失效。

### A3 —— 20081 增量 `/sync` 的 `state_after` 回归

`synapse-storage/src/event/db_tests.rs` 新增  
`state_events_since_batch_keeps_the_last_local_users_leave`：构造"房内最后一个本地用户 leave"，在 leave **之前**截取增量游标，断言 `get_state_events_since_batch` 的结果里含该用户的 `m.room.member`(leave)，且**不含**游标之前的 join（证明下界真的生效）。

**结论：本仓结构上不受 20081 影响。** 根因：`get_state_events_since_batch` 是 `stream_ordering > since` 的**无上界**查询，且**不按当前成员过滤**（`synapse-storage/src/event/state.rs`），所以离开事件必然进入 `state_after`。上游的缺陷来自"取时间线**起点**的状态"这一有界计算，本仓没有该计算。

### 验证

| 命令                                                                                                          | 结果                           |
| ----------------------------------------------------------------------------------------------------------- | ---------------------------- |
| `cargo nextest run -p synapse-services -E 'test(evidence) or test(versions) or test(r0_versions)'`          | **7 passed**（含 3 个红证明）       |
| `cargo nextest run -p synapse-storage -E 'test(state_events_since_batch_keeps_the_last_local_users_leave)'` | **1 passed**（240s，DB 模板克隆较慢） |
| `cargo check -p synapse-storage --all-targets --features test-utils`                                        | exit 0，0 警告                  |

⚠️ 未跑：`tests/integration` 的 `/versions` 快照用例（需要 DB，属重型批次）；该快照已按同一顺序手工同步为 `v1.1`…`v1.17`。

### 连带同步（改版本上限必须一并改，否则制造"既有红"）

1. `tests/integration/snapshots/integration__api_route_snapshots_tests__versions_endpoint.snap`（追加 3 个版本）
2. `AGENTS.md` §"Current external baselines"（`v1.1–v1.14` → **`v1.1–v1.17`**）
3. `docs/synapse-rust/API_COVERAGE_REPORT.md`（标注该报告**尚未**按 v1.15–v1.17 重新取证）
4. `docs/synapse-rust/ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md`（保留历史快照口径，另加一行指针说明上限已移动）
5. `scripts/api_test/response_schemas.json` 是**探针采集存档**，**不手工改**；下次跑 `probe_responses.py` 自然刷新

### A1 记下的缺口（进入批次 B/C 的输入）

- **v1.15**：`GET /v1/room_summary/{roomIdOrAlias}`(MSC3266) **未注册**（本仓只有 `/_matrix/client/v1/rooms/{room_id}/summary`）；OAuth2 认证 API(MSC3861) 未实现；`hierarchy` 的 `allowed_room_ids`/`encryption`/`room_version` 未返回。✅ `GET /v1/auth_metadata`(MSC2965) 已实现。
- **v1.16**：`?format=` on `GET /v3/rooms/{roomId}/state/{eventType}/{stateKey}` **未读取**（handler 无 Query 提取器）；`m.profile_fields` 能力已声明但稳定 `/{keyName}` 路由未注册（既有审计已记录的同一不一致）。✅ MSC4222 已实现（稳定 + 不稳定双拼写）。
- **v1.17**：MSC4210 默认推送规则集未收口（→ 批次 B）；MSC4326/MSC4312/MSC4190 未实现。

---

## 9. 批次 B 实施记录（B1，2026-10-13）

**状态：B1 / B2 / B3 / B4 已实施。**

### 先订正了一处自己的错误

本节所属的 §3.2.2 初稿把 MSC4210 要移除的规则写成 **4 条**（把 `is_user_mention` /  
`is_room_mention` 也算进去了）。按 MSC4210 原文与上游实现  
（`element-hq/synapse` 的 `rust/src/push/mod.rs:635-641`）**正确答案是 3 条**：  
`.m.rule.contains_display_name` / `.m.rule.contains_user_name` / `.m.rule.roomnotif`。  
`is_user_mention` / `is_room_mention` 是**有意提及**规则，**必须保留** —— 按初稿实施会静默  
破坏 `m.mentions`。**教训：清单类结论必须以 MSC 原文/上游实现为准，不能凭规则名望文生义。**

### B1 —— MSC4210 默认隐藏 3 条 legacy 提及规则

| 落点                                                                     | 改动                                                                                                                                                                      |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `synapse-common/src/config/experimental.rs`                            | 新增 `msc4210_enabled`，`#[serde(default = "default_true")]`，默认 **`true`**                                                                                                 |
| `synapse-common/src/push_rules.rs`                                     | 新增 `MSC4210_LEGACY_MENTION_RULE_IDS` 与 `remove_msc4210_legacy_rules()`；两个公开函数的签名加 `msc4210_enabled`                                                                     |
| `synapse-web/src/routes/push_rules.rs` / `push.rs` / `account_data.rs` | 处理器从 `ctx.config.experimental.msc4210_enabled` 取值                                                                                                                       |
| `synapse-services/src/sync_service/{types,mod,data_fetch}.rs`          | `SyncServiceDeps` / `SyncService` 新增 `msc4210_enabled` 字段（`/sync` 的 `m.push_rules` 必须与 `GET /pushrules` 一致）；生产装配在 `wiring/rooms.rs` 读配置，测试用 `new()` 固定为 `true`（与出厂默认一致） |
| `tests/integration/snapshots/…sync_authenticated_initial.snap`         | 同步：`content` 变空数组，`override` 去掉 2 条；**`is_user_mention` / `is_room_mention` 保留**                                                                                        |

**语义说明**：是"隐藏"而非"删除"——`merge_default_push_rules` 对 `default: true` 但已不在  
canonical 集合里的规则会从合并结果中丢弃，用户自定义（非 `default`）规则不受影响。

**⚠️ 客户端可见的行为变更**：旧客户端若依赖服务端下发的 legacy 规则把"显示名命中 / `@room`"  
转成通知，将不再收到该类通知（MSC4210 自述的已知取舍）。需要回退时把  
`experimental.msc4210_enabled` 设为 `false` 即可，无需改代码。

### 验证

| 命令                                                                                                                                                                                                                                      | 结果     |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `cargo check -p synapse-common -p synapse-services -p synapse-web --all-targets --features test-utils`                                                                                                                                  | exit 0 |
| `payload` 级校验：快照 JSON 可解析，规则总数 16，含 0 条 legacy、2 条有意提及规则                                                                                                                                                                                | OK     |
| 新增单测：`msc4210_legacy_id_list_matches_the_msc` / `msc4210_hides_exactly_the_three_legacy_mention_rules` / `msc4210_merge_drops_stored_legacy_rules_but_keeps_custom_ones` / `remove_msc4210_legacy_rules_tolerates_a_malformed_document` | 见提交说明  |

未跑：`tests/integration` 的 `/sync` 快照用例（需 DB，重型批次）——快照已按确定性差异手工同步，  
并做了 JSON 级校验。

### B2 —— MSC4452 稳定能力名 `m.preview_url`

| 落点                                                  | 改动                                                      |
| --------------------------------------------------- | ------------------------------------------------------- |
| `synapse-services/src/capability_governance.rs`     | **无条件**声明稳定能力 `m.preview_url`（与既有别名同值）；`known_keys` 同步  |
| `tests/integration/snapshots/…capabilities_v3.snap` | 按字母序插入 `m.preview_url`（`m.forget_forced_upon_leave` 之后） |

**为什么不照抄上游的开关**：上游 `m.preview_url.enabled = config.media.url_preview_enabled`，  
但本仓的 `MediaStoreConfig` 是**整段注释掉的未实现 stub**，生效的 `UrlPreviewConfig`  
只有 `ip_range_blacklist` —— **没有 media 级启用开关**。若照抄，就会出现"能力报 enabled、  
端点却因 `experimental.msc4452_enabled=false` 返回 403"的自相矛盾。故两者统一取端点实际  
执行的开关。**这是本仓与上游的已知结构差异，记在 §3.2.3。**

未改：端点 403 门控本身（`routes/media/preview.rs`）保持原样 ⇒ 无路由/端点行为变更，  
也就不需要动 `tests/integration/api_media_routes_tests.rs`。

### B3 —— appservice `url: null`（**方案已定：列改为可空**）

**决策**：采用**方案 1** —— 把 `application_services.url` 改为可空，而不是用空串哨兵。

**决策依据（均已实测）**：

- `AGENTS.md:72` 明文："**项目状态：未发布、无外部用户、无生产数据。因此不存在向后兼容义务。**"  
  ⇒ "已应用的迁移不可修改"这条义务在本仓**不成立**，直接改 baseline 是项目认可的路径。
- `docker/db_migrate.sh` 比对基线**内容** md5，内容变化即打印"基线内容已变化，重放基线以应用变更"  
  并**容错重放** ⇒ 改 baseline **对存量库有效**；`docker/deploy/logs/deploy_20261007_090425.log:1852`  
  有该分支真实触发的记录。
- 方案 2 省下的是"改 schema 的麻烦"，换来的是一个隐式约定（`""` = 无推送目标），  
  并让 admin API 与上游语义分叉 —— 不划算。

**实施步骤（有序，每步带验收）**：

1. **baseline**：`migrations/00000000_unified_schema_v12.sql:2468` 的 `url TEXT NOT NULL` → `url TEXT`，  
   并在文件尾部追加一条幂等语句  
   `ALTER TABLE application_services ALTER COLUMN url DROP NOT NULL;`  
   —— ⚠️ **两条都要改**：`CREATE TABLE IF NOT EXISTS` 对已存在的表是 no-op，只有 ALTER 才能  
   把存量库的约束摘掉。
2. **更新指纹**：`tests/unit/test_isolation_unification_tests.rs:190` 的  
   `EXPECTED_BASELINE_FINGERPRINT`（现值 `0de025c8669e0c96`）→ 以测试报出的新值为准；  
   跑该用例确认。**这一步不能忘** —— 它是"基线被字节级改动"的唯一守门人。
3. **存储层**（`synapse-storage/src/application_service/`）：`ApplicationService.url` →  
   `Option<String>`；`RegisterApplicationServiceRequest.url` → `Option<String>`；  
   repository 的 SELECT / INSERT / UPDATE 处理 NULL。
4. **服务层**（`synapse-services/src/application_service/`）：`AppServiceConfigFile.url` →  
   `Option<String>`；`validate_config_file` **接受**显式 `null` / 缺省，仍校验非空 URL 的合法性；  
   `None` 时**不排队**事件与临时数据（**这是 20273 的本体**）；`mod.rs:467` 的 ping 短路。
5. **web 层**（`synapse-web/src/routes/app_service.rs`）：无 url 时 `/_matrix/app/v1/*` 代理  
   **fail-closed** 失败（没有目标主机）；admin 响应里 `url: null`。
6. **测试**：注册一个 `url: null` 的 appservice ⇒ 启动成功、不产生排队行、代理失败、  
   admin 返回 null；并做变异自证（把 `None` 当 `Some("")` 用 ⇒ 用例必须红）。

**⚠️ 取证中发现的隐藏语义冲突（必须一并定掉，否则会做出歧义 API）**：

`UpdateApplicationServiceRequest.url` 现在是 `Option<String>`，语义是"**`None` = 不改**"  
（admin PATCH 用）。若同时让 `None` 表示"置为空"，同一个 `None` 就有两种含义。  
**本轮决议**：admin 更新路径**维持** `None = 不改`；**不支持**通过 admin API 把 url 清空  
（需要清空时用重新加载配置文件的方式）。这条差异会写进代码注释，避免"null 既能表示不改、  
又能表示清空"的歧义。

**受影响面（已实测的消费点）**：推送（`application_service/transaction.rs:237`）、  
ping（`application_service/mod.rs:467`）、AS 代理目标  
（`synapse-web/src/routes/app_service.rs:437`）、admin 响应（同文件 `:705`）。

**不需要动的**：路由字面量不变 ⇒ 无需重生成三张派生路由表/契约产物。

### B3 实施记录（2026-10-14）

| 步骤 | 落点                                                                     | 结果                                                                                                                                                                     |
| -- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1  | `migrations/00000000_unified_schema_v12.sql`                           | `CREATE TABLE` 的 `url TEXT NOT NULL` → `url TEXT`，**并在文件尾部追加**幂等 `ALTER TABLE application_services ALTER COLUMN url DROP NOT NULL;`（照抄 `e2ee_audit_log.device_id` 的先例） |
| 2  | `tests/unit/test_isolation_unification_tests.rs`                       | `EXPECTED_BASELINE_FINGERPRINT` → `5076e5daa3c35b1d`（按 R10 第①条**先用旧值自检**哈希实现，再取新值）                                                                                     |
| 3  | `synapse-storage/src/application_service/{models,repository}.rs`       | `ApplicationService.url` / `RegisterApplicationServiceRequest.url` → `Option<String>`；INSERT 参数改 `as_deref()`                                                          |
| 4  | `synapse-services/src/application_service/{models,mod,transaction}.rs` | 配置模型可空；`validate_config_file` 接受 `null`／拒绝空串（报错指向 `url: null`）；**`enqueue_matching_event` 跳过无 url 的服务（20273 本体）**；ping 与事务投递短路                                         |
| 5  | `synapse-web/src/routes/app_service.rs`                                | `RegisterAppServiceBody.url` → `Option`；`AppServiceResponse.url` → `Option`（admin 返回 JSON `null`）；**代理在无 url 时 fail-closed（503）**                                      |
| 6  | 连带                                                                     | `external_service_integration.rs` 的两处 `is_empty()` 判定改 `is_none()`（原来"无 webhook"被折成空串，现在如实为 `None`）；测试构造点 13 处 + 集成/性能测试 66 处加 `Some(...)`                             |
| 7  | `.sqlx/`                                                               | **必须重生成**（见下）                                                                                                                                                          |

**⚠️ 计划里没写到、实施中才发现的两件事**：

1. **`.sqlx` 离线缓存必须重生成**。appservice 的 CRUD 用的是 `sqlx::query_as!`（编译期校验宏），  
   而 `.cargo/config.toml` 全局设了 `SQLX_OFFLINE=true` ⇒ 列一改可空，缓存里的类型就对不上。  
   重生成的**正确姿势是范围要盖住测试与特性**：  
   `cargo sqlx prepare --workspace -D <url> -- --all-targets --features test-utils --all-features`。  
   只用默认范围会**误删 123 个测试专用查询的缓存**（我实测踩到，已 `git checkout` 还原后重做）；  
   正确范围下只改 **7 个文件**，且正好是 appservice 的 2 INSERT / 4 SELECT / 1 UPDATE。
2. **本机没有可直接用于 `prepare` 的库**。本地 `synapse` 库其实**落后于 baseline**  
   （缺 `media_metadata.content_hash`、缺 invite blocklist 表，实测 52 个无关报错），  
   所以流程是：`createdb synapse_b3_prepare` → `psql -f migrations/00000000_unified_schema_v12.sql`  
   → 用它做 `prepare`。**不要**拿本地 `synapse` 库直接 prepare。

**语义确认**：`update` 用 `url = COALESCE($2, url)`，因此 admin 的 `None` 仍然是"不改"，  
与 §9 的决议一致（不支持经 admin API 清空 url）。

**验证**：

| 命令 / 用例                                                                                                                                                         | 结果                                    |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| `cargo check --workspace --all-targets --features test-utils --all-features`（离线）                                                                                | exit 0                                |
| `cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings`                                                           | exit 0                                |
| `nextest --test unit -E 'test(baseline_fingerprint)'`                                                                                                           | **PASS**（复算值被真实 fixture 认可）           |
| `register_with_null_url_round_trips_as_sql_null`（存储层，DB）                                                                                                        | **PASS** —— 直接断言 `SELECT url IS NULL` |
| `enqueue_skips_appservices_without_a_push_endpoint`（服务层，DB）                                                                                                     | **PASS** —— 含对照组（有 url 的那个必须排队 1 条）   |
| `parse_config_accepts_null_url_as_no_push_endpoint` / `_accepts_omitted_url` / `_rejects_empty_url_and_points_at_null` / `_still_rejects_an_invalid_url`        | 4/4 PASS                              |
| `app_service_response_reports_a_missing_url_as_json_null`                                                                                                       | PASS                                  |
| `check_baseline_consolidation` / `check_migration_consistency` / `check_schema_table_coverage` / `check_schema_contract_coverage` / `check_schema_blind_guards` | 全 exit 0                              |

**变异自证**：把 `enqueue_matching_event` 的判定反转为 `service.url.is_some()` ⇒ 用例红，  
报 `left: 1, right: 0`（无 url 的 AS 被排了 1 条）；还原后绿。

**未覆盖**：代理路由 `/_matrix/app/v1/proxy/{as_id}/{path}` 的 503 分支**没有自动化用例**  
（需要完整 app + DB 的集成夹具）。该分支是 `let-else` fail-closed，代码路径已人工核对，  
但**没有**测试兜底 —— 记在此处，避免被误读为"已测"。

### B4 —— 已核实 + 已实施（2026-10-14）

#### B4.0 核实结论：`/requestToken` 本身**已经不可枚举**，但有一条**真实存在**的旁路

§6 的 V4 已用**端到端真实请求**执行（驱动生产 router，见  
`tests/integration/request_token_enumeration_tests.rs`）：

| 端点                                     | 已注册地址                                 | 未注册地址     |
| -------------------------------------- | ------------------------------------- | --------- |
| `/account/password/email/requestToken` | `200` `{sid, submit_url, expires_in}` | `200`，同形状 |
| `/register/email/requestToken`         | `200`，同形状                             | `200`，同形状 |

原因（结构性，不是"没找到"）：本仓注册 / 绑定 3PID 两条 `/requestToken` **从不检查注册态**，  
而密码重置虽检查、却用**占位会话**（`user_id = NULL`）让响应逐字节一致。  
⇒ **20243 想堵的那个通道本仓结构上就不存在。**

但核实过程暴露了两件必须处理的事：

1. 每条链路都靠"恰好没写错"：**没有开关、也没有测试兜底**。原本声称覆盖它的  
   `tests/unit/account_compat_route_tests.rs` "anti-enumeration contract" 是**纸面测试**（见 B4.2）。
2. `/submitToken` **真的**存在一个可枚举的预言机（见 B4.1）。

#### B4.1 真实存在的枚举通道：`/submitToken` 的**会话存在性**预言机

本仓 `sid` 就是 `email_verification_tokens.id`（**小整数自增**），而  
`validate_and_consume_token` 对"会话不存在"与"token 不符"返回**不同**的 400  
⇒ 客户端可把 `sid` 扫一遍，逐一得知哪些会话真实存在。

上游用 `RegistrationStore._ignore_unknown_session_error` 堵它（同一个  
`request_token_inhibit_3pid_errors` 开关驱动，`synapse/storage/databases/main/registration.py:2710/2896`）。  
本仓按同等语义实现：开关打开时，未知 `sid` 以**与 token 不符完全相同**的错误被拒；  
两分支共用常量 `INVALID_TOKEN_MESSAGE`，防止日后各自漂移回可区分。

**残余（如实记录，与上游同构）**：`is_used` 与"已过期"仍在 token 校验**之前**被拒，  
因此这两类错误与"未知会话"仍可区分。彻底消除需要重排校验顺序，而那会改变  
`claim_used_token`（"已消费 ⇒ 返回上次结果"）这一既有语义 ⇒ **本轮不动**，记录在案。

#### B4.2 顺带修掉一条**不可能失败**的测试

`tests/unit/account_compat_route_tests.rs` 的两条 "anti-enumeration contract" 用例只对  
自造的 `json!` 字面量断言，**从不调用 handler**（末尾甚至只有 `let _ = resolved_user_id;`）  
⇒ 把防枚举逻辑整段删除，它们**依然全绿**。已删除，替换为指向真实覆盖的说明。

真实覆盖落在三处：

| 位置                                                                                     | 覆盖什么                                                                                           |
| -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `synapse-web/src/routes/auth_compat.rs`（内联 `mod request_token_anti_enumeration_tests`） | 直接调用生产谓词 `hides_registration_state` / `honest_request_token_error` / `session_data.purpose` 标记 |
| `tests/integration/request_token_enumeration_tests.rs`                                 | 驱动生产 router：已注册 vs 未注册响应不可区分；开关关闭后恢复规格诚实错误                                                     |
| `synapse-storage/src/email_verification.rs`                                            | 未知 `sid` 与"token 不符"的 `(kind, code, message)` 必须相等                                             |

#### B4.3 实施

| #    | 改动                                                                                     | 文件                                                                                                                                       |
| ---- | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| B4-1 | 新增 `request_token_inhibit_3pid_errors`（`#[serde(default = "default_true")]`，默认 `true`） | `synapse-common/src/config/server.rs`                                                                                                    |
| B4-2 | 三个 `/requestToken` 端点接入闸门：识别本流程的"敏感态"；抑制时返回**同形状**响应且会话**不绑定用户**（故不可兑换）；关闭时返回规格同款诚实错误  | `synapse-web/src/routes/auth_compat.rs`、`.../account_compat.rs`                                                                          |
| B4-3 | 关闭 `/submitToken` 预言机                                                                  | `synapse-storage/src/email_verification.rs`、`synapse-services/src/email_verification_service.rs`、`synapse-web/src/routes/auth_compat.rs` |
| B4-4 | 真实测试 + 删除纸面测试                                                                          | 见 B4.2                                                                                                                                   |

**诚实模式的状态码 / errcode 以规格为准**（不是照抄上游代码）：

- `/account/password/email/requestToken` 未注册 ⇒ `400 M_THREEPID_NOT_FOUND`  
  （`password_management.yaml`，并明确写 `M_THREEPID_IN_USE` **may not** be returned）；
- `/register/email/requestToken` 已占用 ⇒ `400 M_THREEPID_IN_USE`；而  
  `/account/3pid/email/requestToken` 在规格里**引用**该端点（`administrative_contact.yaml`）  
  ⇒ 同一语义。
- ⚠️ 本地既有的 `ApiError::threepid_in_use()` 是 **409**，与本端点规格不符。它服务的是  
  另一条链路，**本轮未改**；新错误显式构造成 400 + `M_THREEPID_IN_USE`。

**一处偏离上游，有意为之**：上游在抑制时返回**随机假 `sid`** 且不落会话；本仓 `sid` 是  
十进制小整数，随机假 `sid` 的**形状本身就是破绽** ⇒ 改为落一个**真实但未绑定用户**的  
会话（响应同形状、`sid` 真实）。不可兑换性由既有下游守卫保证，且是**两道独立**守卫：

- 密码重置：`change_password_uia` 的 `m.login.email.identity` 分支拒绝 `user_id` 为空的会话；
- 3PID 绑定：完成路径先要求 `session_user` 存在（`account_compat.rs:625-628`），  
  再在地址已被他人绑定时由 `add_verified_threepid` 返回 0 行 ⇒ `M_THREEPID_IN_USE`。

**行为变更边界**：默认（`true`）下唯一实质变化是"3PID 绑定一个**已被占用**的地址"时，  
会话不再绑定调用者并因此不可兑换 —— 这正是该开关的语义（上游同样不发放可兑换会话）。  
密码重置与注册两条路径在默认下**逐字节不变**。

#### B4.4 变异自证（红 → 还原 → 绿）

| # | 变异 | 期望红 | 实测 |
| --- | --- | --- | --- |
| M1 | `hides_registration_state` 改为恒返 `false`（"什么都别藏"） | 内联谓词测试 | ✅ FAIL：`a password reset must hide that the address has no account` |
| M2 | `validate_and_consume_token` 的未知会话分支改回"区分消息" | 存储层预言机测试 | ✅ FAIL：`left/right` 恰为两条不同消息（`Invalid session ID or session not found` vs `Invalid verification token`） |
| — | 两处全部还原 | 两条测试回绿 | ✅ 2 passed |

> M1 同时会让 `turning_the_knob_off_restores_the_honest_spec_errors` 变红（该用例断言"关掉开关后必须出现诚实错误"，
> 即"handler 确实咨询了这个谓词"）—— 为省一次全量重链，本轮只实测了它的廉价半边（内联谓词测试）。

#### B4.5 两个必须先知道的陷阱

1. **`ServerConfig` 同时 `derive(Default)`，而 `#[serde(default = "default_true")]` 只在  
   反序列化 YAML 时生效** ⇒ 测试容器（`test_config.rs` 用 `..Default::default()`）拿到的是  
   `false`，与生产**相反**。已在 `test_config.rs` 显式写 `true` 并加注释；否则新增的  
   integration 测试会静默只跑"诚实分支"，看起来一样绿。
2. **本文件曾被外部进程回写为旧版本**（工作区一度变回 244 行的旧稿、`§9` 整体消失，  
   但 HEAD 与索引始终正确）。恢复即可；如需长期避免，应查清是哪一个编辑器/会话持有陈旧缓冲。

#### B4.6 需要决策的相邻项（**本轮未做**）

`GET /register/available` 与 `POST /register` 的 `M_USER_IN_USE` 同样是账号枚举通道，上游有  
**独立**开关 `registration.inhibit_user_in_use_error`（`register.py:367/458/604`，默认 `false`）。  
它**不在**本轮 24 条 changelog 内，故未纳入 B4。本仓 `/register/available` 目前返回真实  
可用性（`auth_compat.rs:120-129`，带注释说明"报 available 反而是谎"）。是否一并收口，待定。
