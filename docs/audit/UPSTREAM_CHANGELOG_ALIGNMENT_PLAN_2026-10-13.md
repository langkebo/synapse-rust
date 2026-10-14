# 上游 Synapse `changelog.d` 对齐优化方案（2026-10-13）

> **数据源**：`element-hq/synapse` @ `develop`，`changelog.d/`（towncrier 未发布条目）。采集时间 2026-10-13。  
> **本仓基线**：`main` @ `ed09854e5`（工作区干净）。  
> **性质**：本文件是**方案**（audit + plan），不改动任何代码。凡"待核实"项均给出一行可复现的核实命令。

---

## 0. TL;DR

> ⚠️ 本节原为 2026-10-13 的**初判**。下面是 **2026-10-14 收口**后的实际处置结果；
> 逐条结论见 §3.1，实施记录见 §8（批次 A）、§9（批次 B）、§10（批次 C）、§11（本轮复核）。

**24 条未发布条目的处置结果**

| 结果 | 条目 | 依据 |
| --- | --- | --- |
| **已实现** | 20333 / 20334（版本宣告）、20220（MSC4210）、20298（MSC4452）、20273（appservice `url: null`）、20243（防枚举）、20323（撤回即失效） | §8 / §9 / §10 |
| **已对齐，仅补测试或已有测试** | 20170（MSC4222 双拼写，`sync.rs:279` 已有测试）、20081（`state_events_since_batch_…`）、20143、20321 | §8 / §3.1 |
| **结构性不适用（附本仓理由，非"没找到"）** | 20006、20202、20302、20312、20315、20328、20295、20322、20324 | §3.1 / §10.2 |
| **未实施 —— 登记为待决策 / 独立特性** | 19354（专用调度端点）、20294（Clock 抽象）、20324（MSC4354 expiry 特性） | §11.3 |

**当前宣告口径**：`/versions` 已宣告 `v1.1`…**`v1.17`**（唯一权威 =
`synapse-services/src/capability_governance.rs` 的 `CLIENT_API_VERSION_SUPPORT`；
本仓有意不含上游的 `r0.0.1`…`r0.6.1`，理由见该常量注释）。

**本仓不适用**：MSC4426 / MSC4259（无 user status 功能）、SQLite→PG 序列（PG-only）。

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
| 20170 | 已支持稳定 + 不稳定双拼写（`synapse-web/src/routes/handlers/sync.rs:237-241`），且**测试已在册**（`msc4222_use_state_after_accepts_both_spellings`，`sync.rs:279`） | **已对齐** | 完成（无需再补测试） |
| 20081                         | 已专项验证（`state_events_since_batch_keeps_the_last_local_users_leave`）                                                                                          | **本仓不受影响** | ✅ 批次 A     |
| 20220                         | 默认规则仍含 **3 条 legacy** 提及规则（`synapse-common/src/push_rules.rs`）                                                                                              | **差距**     | ✅ 批次 B（B1） |
| 20298                         | 只声明不稳定别名 `io.element.msc4452.preview_url`，缺稳定名（`synapse-services/src/capability_governance.rs`）                                                             | **差距**     | ✅ 批次 B（B2） |
| 20295 / 20322 / 20323 / 20324 | 有 MSC4354 实现（`synapse-web/src/routes/room.rs:82-95,169`）。逐条判定：**20323 已修复**（撤回即失效，§10.1）；**20295 N/A**（本仓 sticky 不注入 timeline，且本仓根本没有历史可见性过滤，§10.2）；**20322 / 20324 无对应形态**（无"持久化批次"概念、无 expiry 字段） | **已收口** | 批次 C（完成） |
| 19354 / 19495 | 发送端 `delay` **查询参数**已修（`handlers/room/mod.rs`，§10 的 C2）；管理端点 `GET/POST .../delayed_events/{delay_id}`（`routes/delayed_events.rs:98`）+ storage + 联邦 EDU。19495 的隐式取消**结构性不存在**：唯一取消入口是显式 `action: cancel`，且按 `id` 单键 | **部分对齐** | 见 §11.3 —— **19354 的专用调度端点未实现** |
| 20273 | `url` 已改 `Option<String>`，`None` 时**跳过**事件/临时数据排队（§9 的 B3） | **已实现** | ✅ 批次 B（B3） |
| 20243 | `ServerConfig.request_token_inhibit_3pid_errors`（默认 `true`）已落地；实测另有**真正的**枚举通道 —— `/submitToken` 的会话存在性预言机，已一并关闭（§9 的 B4 / B4.7） | **已实现** | ✅ 批次 B（B4 / B4.7） |
| 20006                         | 全局 body 上限由路由层前置拦截（`src/server/router.rs:37-39`）⇒ 超限在落盘前 400/413                                                                                            | 不适用        | 无          |
| 20321                         | `ApiError::message()` 对 `Internal` 返回泛化文案，细节只进日志（`synapse-common/src/error.rs:888-900`）                                                                     | 已对齐        | 无          |
| 20202                         | 本仓无"配置禁用 server-default 推送规则"的开关                                                                                                                            | 不适用        | 无          |
| 20143                         | `.well-known/matrix/client` 仅回 `m.homeserver` / `m.tile_server`（`synapse-web/src/routes/handlers/versions.rs:142-154`）                                      | 已对齐        | 无          |
| 20302                         | 本仓 PG-only，无 SQLite 迁移路径                                                                                                                                    | 不适用        | 无          |
| 20312 | 本仓无事件级内存元数据结构（`EventInternalMetadata` 不存在）；`received_ts` 仅存在于 `event_reports` 表与 `EventReport` 结构（`synapse-storage/src/event/models.rs:291`），不是事件内部元数据字段 | **架构差异（无对应形态）** | 无 |
| 20315 | 本仓**没有服务端推送规则评估引擎**（`set_tweak` 只出现在默认规则字面量里，`push/service.rs` 的 "rule evaluation" 只在文档注释中）⇒ 没有"逐事件评估并跳过 sender"这一步 | **结构性不适用** | ✅ 批次 C（C3） |
| 20328                         | 本仓无 MSC4426 User Status 功能                                                                                                                                  | 不适用        | 无          |
| 20294 | 生产侧无统一 `Clock` 抽象（`routes/rendezvous.rs:208,213` 与 `storage/rendezvous.rs` 直接调 `current_timestamp_millis()`）；但**测试替身已注入时钟**（`test_mocks/rendezvous.rs:42` 的 `clock: Arc<Mutex<i64>>`，读到即自增）⇒ rendezvous 的时间可控性已具备 | **等价物已存在** | 不重构（§11.3） |
| 20331 | 上游是**纯文档**条目。本仓同样成立且机制已在：`/auth_metadata` 落在 `WorkerType::Frontend` 的 `owned_route_prefixes = ["/_matrix/client/*"]` 内 ⇒ 可由 Frontend worker 服务；本轮补文档（`docs/synapse-rust/MSC_SEMANTICS.md` 的 MSC2965 行） | **已对齐（补文档）** | ✅ 本轮 |

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

> **执行状态（2026-10-14）**：批次 A（A1–A3）、批次 B（B1–B5）、批次 C（C1–C3）已全部实施并分提交入库；
> C4 判「等效物已存在，不重构」（§3.1 的 20294 行）、C2 的 19354 专用端点登记为待决策（§11.3）；
> 批次 D 的 `docs/audit/INDEX.md` 已登记本文件，`CHANGELOG.md` 按"发布时成批撰写"的既有惯例本批不追加。

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

## 6. 待核实项（附可复现命令与其结论）

| # | 待核实的问题 | 核实命令 / 方法 | 结论 |
| -- | --- | --- | --- |
| V1 | 上游 v1.15/v1.16/v1.17 相对 v1.14 新增了什么 | 读 `matrix-spec` 对应版本的 changelog；或对比上游的版本宣告表 | ✅ 已核实（批次 A）。**口径订正**：上游的版本列表已迁到 **Rust** 侧 `rust/src/handlers/versions.rs`，不再是 Python 的 `synapse/rest/client/versions.py` |
| V2 | MSC4452 稳定化是否改默认开关 | 读上游 PR #20298 与其 `experimental` 配置 diff | ✅ 已核实（B2）：稳定名 `m.preview_url` 的 `enabled` 取自 `media.url_preview_enabled`；本仓无 media 级开关 ⇒ 稳定名与别名同取端点实际执行的 `experimental.msc4452_enabled` |
| V3 | MSC4140 是否需要"专用端点" | 读上游 PR #19354 的 REST 路由 diff，比对本地 `delayed_events.rs` | ✅ 已核实（C2）：**需要，且本仓未实现** —— 上游新增 `PUT .../org.matrix.msc4140/rooms/{roomId}/delayed_event/{eventType}/{txnId}` 并**弃用** `/send`、`/state` 上的 `delay` 查询参数。本地已把查询参数修好（§10 C2），专用端点登记为待决策（§11.3） |
| V4 | 本地 `/requestToken` 是否泄露账号存在性 | 对已注册/未注册邮箱各发一次 `/account/3pid/email/requestToken`，比较响应 | ✅ 已核实（B4）：三条 `/requestToken` 均不可枚举；实测另有**一条真实通道** —— `/submitToken` 的会话存在性预言机，已关闭（B4.7） |
| V5 | 本地推送评估是否跳过 sender | `git grep -n "sender" -- synapse-services/src/push synapse-services/src/sync_service` | ✅ 已核实（C3）：本仓**没有服务端推送规则评估引擎** ⇒ 该优化无对应对象 |
| V6 | 20315 的提速是否适用本仓 | 同 V5 | ✅ 同上（结构性不适用） |
| V7 | `/auth_metadata` 能否由 worker 服务 | `synapse-web/src/routes/worker.rs` 的门控清单是否含该路径 | ✅ 已核实（本轮）：它不在 `DELAYED_EVENTS_WORKER_PATHS`（该集合**只**列 MSC4140 族），但**已被 `WorkerType::Frontend` 的 `/_matrix/client/*` 前缀覆盖** ⇒ 可由 Frontend worker 服务；本轮补文档 |

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

## 9. 批次 B 实施记录（B1–B5，2026-10-13 → 2026-10-14）

**状态：B1 / B2 / B3 / B4 已实施；B4.7（残余预言机）与 B5（`inhibit_user_in_use_error`）为 2026-10-14 追加。**

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

**残余（原先与上游同构）**：`is_used` 与"已过期"原先在 token 校验**之前**被拒，  
因此这两类错误与"未知会话"仍可区分。彻底消除需要重排校验顺序，而那会改变  
`claim_used_token`（"已消费 ⇒ 返回上次结果"）这一既有语义 ⇒ **已在 B4.7 关闭**（重排校验顺序，把 token 比对提到最前；`claim_used_token`  
不受影响）。

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

#### B4.6 相邻项：`inhibit_user_in_use_error`（**已实施，见 B5**）

`GET /register/available` 与 `POST /register` 的 `M_USER_IN_USE` 同样是账号枚举通道，上游有  
**独立**开关 `registration.inhibit_user_in_use_error`（`register.py:367/458/604`，默认 `false`）。  
它**不在**本轮 24 条 changelog 内。**已按上游语义实施，见 §9 的 B5 节。**

#### B4.7 残余预言机已关闭（追加，2026-10-14）

B4.1 记录的"生命周期仍可区分"是**真的可利用**：拿垃圾 token 探一个既有 `sid`，在 token  
比对之前就会拿到 `has already been used` / `has expired`，而这两条与未知会话的消息不同 ——  
于是自增的 `sid` 仍能被逐个归类。

修法是**把 token 比对提到所有状态检查之前**（`synapse-storage/src/email_verification.rs`
的 `validate_and_consume_token`）：

| 原顺序 | 新顺序 |
| --- | --- |
| 1 行存在性 → 2 `is_used` → 3 过期 → 4 token 比对 → 5 client_secret | 1 行存在性 → **2 token 比对** → 3 `is_used` → 4 过期 → 5 client_secret |

- 于是**未知 sid / 已用过 / 已过期**对"不知道 token"的调用方给出**完全相同**的
  `INVALID_TOKEN_MESSAGE`；而"已用过/已过期"只对**持有 token** 的调用方可见 —— 那正是
  合法客户端的场景，可调试性没有被牺牲。
- **不改** `claim_used_token`（"已消费 ⇒ 返回上次结果"），其语义与顺序无关。
- 重排是**无条件**的；`inhibit_unknown_session=false` 时只影响"未知 sid"那一条消息。
- **无新增查询**：行查找对命中/未命中都是同一条 `SELECT`，顺序变化不引入查询条数差异。
- 回归守卫：`session_lifecycle_is_indistinguishable_from_an_unknown_sid`
  —— 用垃圾 token 分别探"未知 / 已用过 / 已过期"，断言四条错误**彼此相等**（不是断言字面量）。

> 与上游的关系：上游在该处仍是"状态先于 token"，所以本仓在这一条上**比上游更严**。
> 这是有意的：上游的 `_ignore_unknown_session_error` 只遮了未知会话，没遮生命周期。

---

### B5 —— `/register/available` 成员存在性探针（枚举抑制，2026-10-14）

**非 changelog 项**（上游独立开关，不在本轮 24 条之内），按上游语义一并收口。

#### B5.0 上游语义（逐字取自上游配置文档）

`registration.inhibit_user_in_use_error`，**默认 `false`**：

> 开启后 `/register/available` 会把**任何**用户名都报成可用，且 `/register` **开始**一次
> 已存在用户名的注册时不再报错；但**注册完成时**用户名冲突**仍会**报错。

#### B5.1 本仓对应面（实测）

| 上游触点 | 本仓现状 | 结论 |
| --- | --- | --- |
| `GET /register/available` 早退返回 `available: true` | `auth_compat::check_username_availability` 用 `user_exists` 真实探库 | **需要改** |
| `POST /register` 的"开始注册"预检（上游 `check_username`） | **不存在**：`M_USER_IN_USE` 只来自完成时 `create_user` 的唯一键冲突（`synapse-services/src/auth/register.rs:83-90`） | **无需改** —— 两条默认值下行为一致，且这正是上游明确**不**抑制的那一种 |

#### B5.2 实施

- `ServerConfig.inhibit_user_in_use_error: bool`（`#[serde(default)]`）。⚠️ 与
  `request_token_inhibit_3pid_errors` 相反：`bool` 的 `Default::default()` 恰为 `false`，
  与 serde 默认**一致** ⇒ **不存在**"YAML 缺省 vs 测试容器 `..Default::default()`"
  取值相反的那个陷阱（B4.4 第 1 条）。
- 开启时 `/register/available` **不再查库**，直接回 `{"available": true, "username": …}`。
- **保留** `validate_username` 语法检查（与上游"先解析后短路"的顺序略有差异，**有意**）：
  非法 localpart 无论谁存在都报 400，不泄露任何信息；保留它让端点契约不随开关变化。

#### B5.3 这是一项折中，不是完整修复（重要）

即使开启，`POST /register` **完成时**的冲突**仍会**报错（上游亦然，由 `users.user_id`
唯一索引决定），客户端依旧能用"试着注册"来区分。它挡掉的是那条**廉价、无副作用**的
探测路径。两处都已在代码注释里写明，避免被误读为"已彻底防枚举"。

#### B5.4 验证

`tests/integration/register_enumeration_tests.rs`（驱动生产 router）：

- 关闭（默认）⇒ 已注册名报 `available: false`、未用名报 `true` —— 这是**基线**，
  没有它，第二条用例即使对着一个"恒返回 true"的空实现也会绿；
- 开启 ⇒ 两者**都**报 `true`，且响应**形状**一致（仅 `username` 回显不同）。

---

## 10. 批次 C 实施记录（C1/C2/C3，2026-10-14）

### C3 —— 20315（推送规则评估跳过 sender）：**结构性不适用**

本仓**没有服务端推送规则评估引擎**：rules 只被存储与下发（`/pushrules` 的 CRUD +
sync 的 `m.push_rules` 账号数据），`set_tweak` 只出现在 `synapse-common/src/push_rules.rs`
的默认规则**字面量**里，投递走 `/_matrix/vendor/v1/push/send`（调用方自带通知内容）。
`synapse-services/src/push/service.rs` 里"push rule evaluation"只存在于文档注释中，配套的
`get_ignored_users` **没有任何调用者**。没有"逐事件评估规则"这一步，也就没有可跳过的
sender 检查 —— 不是"没找到"，是没有这个计算。

### C2 —— MSC4140 的 delay 参数位置：**已修复**

#### C2.1 缺陷（实测）

上游把 delay 读作**查询参数**。下面是 `element-hq/synapse@develop` 的
`synapse/rest/client/room.py`：

```python
def _parse_request_for_delayed_event_delay(request, msc4140_enabled) -> Duration | None:
    delay_param_name = "org.matrix.msc4140.delay"
    delay_ms = parse_integer(request, delay_param_name, negative=True)
```

本仓此前只读**请求体**同名字段，且四个写路径都没有 `Query` 提取器：

| 调用点 | 结果 |
| --- | --- |
| `handlers/room/events.rs` `send_message`（`/send`） | 无 `Query` ⇒ 查询串里的 delay 完全不可见 |
| `handlers/room/state.rs` `send_state_event` / `put_state_event` / `put_state_event_empty_key` / `put_state_event_no_key` | 同上 |

后果具体：按 MSC/上游实现的客户端发 `?org.matrix.msc4140.delay=30000`，本仓**忽略延迟、
立即投递** —— 客户端以为还能撤回，消息已经出去了。上游的调用点只有两个 servlet，与本仓
这四个调用点一一对应。

#### C2.2 修法

- 新增常量 `MSC4140_DELAY_QUERY_PARAM` 与纯函数 `msc4140_query_delay_ms()`
  （`handlers/room/mod.rs`）。解析规则对齐上游 `parse_integer(..., negative=True)`：
  非整数值 ⇒ 400 `M_INVALID_PARAM`（**不是**静默忽略）；负值**放行**到下游的 `> 0` 检查，
  使 `0` / `-1` / `-99999` 得到**同一条**错误（上游同样用这个拆分保证一种错误对应一种错误）。
- `schedule_delayed_event_if_requested()` 增加 `query: &HashMap<String, String>` 参数，
  取值 **query 优先、body 兜底**。body 是"旧拼写"，保留它是因为删除会**静默改变既有
  客户端的时序**（本该延迟的变成立即投递）。
- 四个写路径 + `put_power_levels` 委托点补 `Query<HashMap<String, String>>` 提取器
  （`Query` 放在 `Json` **之前** —— 提取器按声明顺序求值，只有 `Json` 消费 body，必须最后），
  并把查询表透传给共享函数。
- 既有 `> 0` / `≤ 24h` 两条校验**原样保留**。⚠️ 已知的**外观性**差异：这两条返回的是
  400 `M_BAD_JSON`（`ApiError::bad_request`），而上游对非正值也用 `M_INVALID_PARAM`。
  本轮只给新增的"非整数"用 `M_INVALID_PARAM`，是因为"保留既有校验"优先于"统一 errcode"；
  若要统一，是 `mod.rs` 里两处 `ApiError::bad_request` → `ApiError::invalid_input` 的一行改动。

#### C2.3 为什么用动态 `Query<HashMap<..>>` 而不是具名结构体

契约链的 `query_params` 真源是 `scripts/contract/ledger_annotations.txt`；具名
`Query<Struct>` 会被反向守卫判为 `ok`（字段名可解析），从而**必须**同步注解表 +
脚本内 `EXPECTED_ANNOTATIONS` + 三张 `derived_route_table_*.inc.rs` + 两份 fixtures +
`route-table.json` + 集成快照 —— 这正是本仓最高频的"既有红"形态。动态类型被守卫显式
归类为 `manual`（`_NON_STRUCT_QUERY_TYPES` 含 `HashMap`）并"刻意不介入"，因此本次修复
**不动契约链**（已实测 `bash scripts/contract/check_route_contract.sh` exit 0）。代价是
参数名不进 ledger —— 由本节的文字与下面的测试承担记录。仓内已有先例：
`routes/key_rotation.rs` 的两处查询处理器就是 `Query<HashMap<String, String>>`。

#### C2.4 验证

`tests/integration/api_delayed_state_event_tests.rs`（驱动生产 router + 真实 DB）：

| 用例 | 断言 |
| --- | --- |
| `test_query_delay_schedules_a_state_event` | `?…delay=60000` 且 body 无该字段 ⇒ 有 `delay_id`，库内 `delay_ms = 60000`，管理端点可见且内容已剔除该字段 |
| `test_query_delay_schedules_a_message_event` | `/send/…?…delay=30000` ⇒ 有 `delay_id`，库内 `30000` |
| `test_query_delay_wins_over_the_legacy_body_field` | query=60000 且 body=1000 ⇒ 库内 **60000**（直接读 `delayed_events.delay_ms`，而不是回读请求） |
| `test_query_delay_is_validated` | `abc` / 空值 / `0` / `-1` / `86400001` ⇒ 全部 400（`abc` 额外断言 `M_INVALID_PARAM`），并附**对照**：同一路由的合法值仍能调度 |
| `test_absent_delay_hint_still_sends_immediately` | 无任何 hint ⇒ 返回 `event_id`、无 `delay_id` |
| 既有 4 条 body 形式用例 | 原样通过 ⇒ 兜底路径未回归 |

**变异自证**（红 → 还原 → 绿）：

- **M1** 让 `msc4140_query_delay_ms()` 恒返回 `Ok(None)` ⇒ 前三条红，而
  `test_absent_delay_hint_still_sends_immediately` 保持绿（归因干净：红的是被改的那条路径，
  不是"测试写错"）；
- **M2** 把 `query_delay_ms.or(body_delay_ms)` 反转为 `body_delay_ms.or(query_delay_ms)`
  ⇒ 只有"query 优先"那条红。

#### C2.5 顺带查实（**本轮未做**）

上游在 `_parse_request_for_delayed_event_delay` 里还有一个 `msc4140_enabled` 开关：未开启时
请求延迟事件返回 **403 `M_FORBIDDEN`**。本仓**没有**这个开关（延迟事件恒开启），按"未实现
即不宣告"的口径不构成不一致；若要支持，需要新增配置项 + 403 分支，属独立改动。

### C1 —— MSC4354 四条修复（2026-10-14）

本仓的 MSC4354 与上游**不是同一个模型**，判定必须按本仓模型做，不能照搬上游的措辞：

| 维度 | 上游 Synapse | 本仓 |
| --- | --- | --- |
| sticky 是什么 | **事件自身的属性**：发送时带 `sticky_duration_ms`，服务端把 TTL 写进事件的 `unsigned` | **按用户的置顶行**：`room_sticky_events(room_id, user_id, event_type, event_id, is_sticky)`，由客户端 `POST /rooms/{room_id}/sticky_events` 显式设置 |
| 到期 | 有 `expires_at` 列 + 后台清理任务 | **无 expiry 概念** |
| 下发形态 | sticky **事件本身留在 timeline** 里 | 只下发一个**旁的元数据数组** `sticky_events: [{event_type, event_id, is_sticky}]`，**不注入 timeline** |

逐条判定：

- **20322**（同一持久化批次内被 redact ⇒ 不应 sticky）：**无对应形态**。本仓没有"同一批次持久化"这一步，
  sticky 由独立的客户端调用设置，不存在"批次内先置顶、后撤回"的竞态。
- **20323**（持久化**之后**被 redact ⇒ 移除 sticky）：**真实缺口，已修复** —— 见 §10.1。
- **20324**（到期上报值与服务端追踪值不一致）：**无对应形态**。本仓没有 expiry 字段，也就没有"上报值"，
  属 MSC4354 的 expiry **特性缺失**（特性缺口，不是 bug 修复）。
- **20295**（历史可见性豁免覆盖全部 C-S 端点）：**结构性不适用（N/A）** —— 见 §10.2。

#### 10.1 20323 —— 撤回即失效（已修复）

**缺陷**：`room_sticky_events` 只按 `event_id` 记录置顶，**没有任何一步在事件被撤回时清理它**。
撤回之后 `get_is_sticky_event` / `get_all_is_sticky_events` / `get_rooms_with_is_sticky_events`
仍会返回该行 ⇒ `/sync`、sliding sync 与 `GET /rooms/{room_id}/sticky_events` 会继续把一个
"内容已被抹掉的事件"报成置顶。

**修法（与上游同款：写入时清理，而不是读时过滤）**：在 `EventStorage::redact_event_content`
（`synapse-storage/src/event/redaction.rs`）里，把"改写事件内容"与"删除该事件的置顶行"放进
**同一个事务**（本仓硬规则：多语句写必须 `begin → execute ×N → commit`）。

选写入时清理而不是读时过滤，理由是**可遗忘面**：撤回有四个调用点（客户端 `/redact`、
MSC3912 级联、管理端批量、worker 延迟撤回），读路径有五个（单条 / 全量 / 按房间 / sync /
sliding sync）。把不变量放在**唯一的撤回汇聚点**上，未来新增撤回路径不可能漏掉；读时过滤则
要求每一个新读路径都记得加 JOIN，漏一个就是一个静默的错。上游也是这个选择：
`synapse/storage/databases/main/sticky_events.py::delete_sticky_events_txn`（其文档字符串写着
"This is used when an event is redacted"），由事件持久化路径调用。

**为什么这里是唯一的跨模块写者**：`room_sticky_events` 的其余写者都在 `sticky_event.rs`。
本条 DELETE 刻意写在撤回路径上——正是"只有一个汇聚点"才使得它不可能被忘掉。
`redact_event_content` 确实是 `events` 表被置为已撤回的**唯一**入口（本 crate 里另外两处
`is_redacted = TRUE` 写的是 `thread_replies` 与 `event_relations`，不是 `events`）。

**删除按 `event_id` 单独作键（跨用户、跨房间）**：事件被撤回是对**所有人**生效，只清撤回者的行会
让其余用户的置顶变成同一个 bug 换个位置。上游同理（其 DELETE 也不带用户维度）。
另外上游是 DELETE 而不是"把 `is_sticky` 置 false"，本仓跟随 DELETE：置顶本身不再存在。

**刻意不加 `(event_id)` 索引**：该表每个 `(room_id, user_id, event_type)` 只有一行
（`ON CONFLICT DO UPDATE` 原地更新），行数上界是 用户 × 房间 × 事件类型，远小于事件表规模；
而上游的 `sticky_events` 是**追加型**表，这正是他们需要绕道 `events.stream_ordering` 才敢删的原因。
若将来该表的增长性质改变，这条决定需要重新评估。

**验证**：`synapse-storage/src/sticky_event.rs` 的 `db_tests::test_redacting_an_event_removes_its_stickiness`
驱动**真实的** `EventStorage::redact_event_content`（而不是手写 `UPDATE room_sticky_events` ——
后者把修复回退掉也照样绿），并带一个**对照**：另一个未被撤回的事件必须仍然置顶，
否则"删掉全部置顶行"的变异也会看起来正确。该测试位于 `--workspace --lib --all-features` 车道，
CI 的 `cargo nextest run --workspace --lib --all-features` 会真跑它。

#### 10.2 20295 —— 历史可见性豁免：**N/A**（附带一个更大的发现）

上游 `tests/rest/client/test_sticky_events.py::test_joined_history_visibility_messages_returns_sticky_events`
给出了这条修复的确切语义：在 `history_visibility: joined` 的房间里，后加入的用户 backfill `/messages`
时**看不到**加入前的普通消息，但**看得到**加入前的 sticky 事件（测试注释原文：
"the sticky event (which is exempt from history visibility)"）。20295 修的是"这条豁免原先只在部分端点生效"。

本仓判 **N/A**，两条独立理由，每条都可复现：

1. **本仓的 sticky 不进入 timeline**。它只作为房间对象上的 `sticky_events` 元数据数组下发：
   `synapse-services/src/sync_service/response.rs:242-262` 的 `obj.insert("sticky_events", …)`、
   `synapse-services/src/sliding_sync_service/filters.rs:223-244` 的 `room_json["sticky_events"] = …`，
   以及 `GET /rooms/{room_id}/sticky_events` 返回的 `{events:[{room_id,user_id,event_id,event_type}]}`。
   没有任何路径因为"某个事件是 sticky"而改变**事件本身**是否被返回 ⇒ 没有可豁免的对象。
2. **本仓没有历史可见性过滤**。`history_visibility` 在 Rust 源码里只出现在四类位置，全是存储或元数据：
   存列与读列（`rooms` / `room_summaries`）、`GET /rooms/{roomId}` 回显、`world_readable` 派生位
   （目录与搜索用）、以及作为**鉴权事件类型**与 **redaction 保护字段**。事件读取路径
   （`synapse-storage/src/event/reader.rs` 的分页查询、sync 的 timeline 构造）里**一句可见性判断都没有**。

复现命令（第二条若只剩存储用途，即证明上述第 2 点）：

```bash
grep -nE 'visib|membership|joined' synapse-storage/src/event/reader.rs
git grep -n 'history_visibility' -- '*.rs' | grep -vE 'm\.room\.history_visibility|INSERT INTO rooms|room_summar'
```

⚠️ **由此暴露一个比 20295 大得多、且与本轮无关的真实缺口**：`m.room.history_visibility`
**被存储、被回显、被当作鉴权事件保护，但从未在任何读端点上强制执行**。也就是说
`GET /rooms/{roomId}/messages`、`/context/{eventId}`、`/event/{eventId}`、`/sync` 目前对
"加入前历史"不做任何裁剪。这不属于 20295 的范围（20295 是给 sticky 开豁免，而不是实现可见性），
而是一条独立的、跨多个端点的实现缺口。本轮**只做取证与记录，未实施**：实施前需要先定义
"谁在什么时刻能看到"（成员事件的 stream ordering 与事件 stream ordering 的比较规则）并逐端点接入，
属独立批次。记录在此是为了避免它被 20295 的 N/A 结论顺带盖过去。

---

## 11. 本轮（2026-10-14 第二次）复核与剩余问题清单

### 11.1 本轮做了什么

- 复核 24 条条目并给出**终态**（§0 的处置表 + §3.1 的逐条行）。
- 补齐两处此前只写"待核实"的判定：**20294**（Clock 抽象）与 **20331**（`/auth_metadata` 的 worker 可服务性）。
- 订正两处**过时判定**：20170 的"仅补测试"（测试其实已在册：`msc4222_use_state_after_accepts_both_spellings`）、20312 的 `received_ts` 归属（它属于 `event_reports` / `EventReport`，不是事件级内部元数据）。
- 批次 D：`docs/audit/INDEX.md` 早已登记本文件；`CHANGELOG.md` 按既有惯例**不按提交追加** —— 该文件的条目按**发布版本**成批撰写（当前最新条目为 v6.2.0 / 2026-07-22），且没有任何工作流或脚本要求提交时更新它（`git grep CHANGELOG .github/workflows scripts tests` 为空）。

### 11.2 复核用的可复现命令

```bash
git grep -n 'msc4222.use_state_after' -- synapse-web/src/routes/handlers/sync.rs
git grep -n 'owned_route_prefixes' -- synapse-storage/src/worker/tests.rs
git grep -n 'clock' -- synapse-storage/src/test_mocks/rendezvous.rs
git grep -n 'received_ts' -- synapse-storage/src/event/models.rs
git grep -n 'CHANGELOG' -- .github/workflows scripts tests/unit
```

### 11.3 仍未做 / 待决策（按建议优先级）

1. **19354：MSC4140 专用调度端点** —— 唯一仍未对齐的**产品**面。

   上游 `synapse/rest/client/room.py::RoomDelayedEventRestServlet`（PR #19354，merged 2026-10-08）：

   ```text
   PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{roomId}/delayed_event/{eventType}/{txnId}
   body: { delay_ms: PositiveInt, content: object,
           state_key?: string, "org.matrix.msc4354.sticky_duration_ms"?: int }
   → 200 { "delay_id": "..." }
   ```

   上游同时**弃用** `/send` 与 `/state` 上的 `delay` 查询参数（PR 原文："This should deprecate the `delay` query parameter on the `/send` and `/state` endpoints."），但**未移除** ⇒ 本仓 C2 的修复已让"查询参数"这条通路可用，缺的只是"只认专用端点"的新客户端。

   另有三条上游管理子路径与一个列表端点本仓没有：`POST .../delayed_events/{delay_id}/{cancel,send,restart}` 与 `GET .../delayed_events`。功能上已由本地 `POST .../delayed_events/{delay_id}` 的 `action` 字段覆盖 ⇒ 属**形态差异**，不是能力缺失。

   **成本**：新增一条路由必须连带重生成契约链 —— 三张 `derived_route_table_*.inc.rs`、两份 fixtures + `manifest.json` 指纹、`docs/openapi/route-table.json`、`ROUTE_CONTRACT.md`、`route_ledger_*.snapshot`、以及 `docs/synapse-rust-vs-synapse-comparison.md` 的注册路由计数。另需决定是否把新路径加入 `DELAYED_EVENTS_WORKER_PATHS`：Guard B 用 `path.contains("delayed_events")` 过滤，而该路径是**单数** `delayed_event` ⇒ 不加也不会红，但语义上属同族（建议加入）。

   **建议**：等 MSC4140 的 unstable 形态稳定后一次性做（上游自己仍在该 MSC 的 PR 之间调整），避免为一个仍在变动的路径付契约链重生成的全价。

2. **20324：MSC4354 的 expiry 特性**（不是 bug）。本仓 sticky 无 `expires_at`/TTL 概念 ⇒ 属**特性缺口**；上游那一条修的是"上报值与服务端追踪值不一致"，在本仓无对应对象。是否引入 TTL 是产品决策。

3. **20294：统一 `Clock` 抽象**（可选项）。测试替身已注入时钟（`test_mocks/rendezvous.rs:42` 的 `clock: Arc<Mutex<i64>>`，读到即自增），生产侧没有统一抽象。若将来要断言 `routes/rendezvous.rs` 生成的 `message_id` / `sent_ts` 的确定性，才需要引入；否则纯属重构。

4. **历史可见性强制执行** —— **由 §10.2 的 20295 判定顺带发现，比 20295 大得多**：`m.room.history_visibility` 被存储、被回显、被当作鉴权事件保护，但**从未在任何读端点上强制执行** ⇒ `GET /rooms/{roomId}/messages`、`/context/{eventId}`、`/event/{eventId}`、`/sync` 对"加入前历史"不做任何裁剪。需要先定义"谁在什么时刻能看到"（成员事件的 stream ordering 与事件的 stream ordering 的比较规则），再逐端点接入。

### 11.4 与本方案无关、但复核中确认的两条环境事实

- `grep` 在本机被 toybox 替换：**BRE 扩展 `\|` / `\+` 会静默返回空**，看起来像"确实没有匹配"。本文件的所有检索结论都应视为"用 `Grep` 工具（ripgrep）或 `grep -E` 得到"；本轮就因此误判过一次（"tests 里没有延迟事件测试"）。
- `docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` 会被一个 **markdown 表格格式化器**反复回写（表格补空格填充、行尾硬换行、注入连续空行触发 MD012，并曾整段吞掉新章节）。提交前应核对索引快照（`git show :<path>`），提交后核对 `git show HEAD:<path>` 的行数与章节标记。
