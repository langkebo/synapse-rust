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
| **结构性不适用（附本仓理由，非"没找到"）** | 20006、20202、20302、20312、20315、20328、20295、20322 | §3.1 / §10.2 |
| **本轮新补（2026-10-14 第三轮）** | 20324（MSC4354 sticky expiry —— 原判"特性缺口"，现已实现） | §11.5 |
| **未实施 —— 登记为待决策 / 独立特性** | 19354（专用调度端点，方案见 §11.7）、20294（Clock 抽象） | §11.7 / §11.3 |

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
| 20295 / 20322 / 20323 / 20324 | 有 MSC4354 实现（`synapse-web/src/routes/room.rs:82-95,169`）。逐条判定：**20323 已修复**（撤回即失效，§10.1）；**20295 N/A**（本仓 sticky 不注入 timeline，且本仓根本没有历史可见性过滤 —— 该缺口已于 §11.6 部分收口，§10.2）；**20322 无对应形态**（无"持久化批次"概念）；**20324 已实现**（sticky expiry，上报值＝追踪值，§11.5） | **已收口** | 批次 C + §11.5 |
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
- **20324**（到期上报值与服务端追踪值不一致）：**2026-10-14 第三轮已实现**。原判"无对应形态"是对"缺口"的正确描述，但本轮把该缺口补上了 —— 见 §11.5。
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

   **建议**：等 MSC4140 的 unstable 形态稳定后一次性做（上游自己仍在该 MSC 的 PR 之间调整），避免为一个仍在变动的路径付契约链重生成的全价。**2026-10-14 第三轮复核后的结论不变，但已给出可直接照做的实施方案（含 handler 设计、契约链重生成清单、以及一个必须先定的产品决策）→ §11.7。**

2. ~~**20324：MSC4354 的 expiry 特性**~~ —— **2026-10-14 第三轮已实施**，见 §11.5（不再是待决策项）。

3. **20294：统一 `Clock` 抽象**（可选项）。测试替身已注入时钟（`test_mocks/rendezvous.rs:42` 的 `clock: Arc<Mutex<i64>>`，读到即自增），生产侧没有统一抽象。若将来要断言 `routes/rendezvous.rs` 生成的 `message_id` / `sent_ts` 的确定性，才需要引入；否则纯属重构。

4. ~~**历史可见性强制执行**~~ —— **2026-10-14 第三轮已实施 6 条读路径**，见 §11.6。`/notifications` 判为**结构性不适用**、unread 计数另有一处缺口（均见该节表格）；**该缺口与同一轮发现的第三条泄漏路径已于第四轮收口 → §11.10**。

### 11.4 与本方案无关、但复核中确认的两条环境事实

- `grep` 在本机被 toybox 替换：**BRE 扩展 `\|` / `\+` 会静默返回空**，看起来像"确实没有匹配"。本文件的所有检索结论都应视为"用 `Grep` 工具（ripgrep）或 `grep -E` 得到"；本轮就因此误判过一次（"tests 里没有延迟事件测试"）。
- `docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` 会被一个 **markdown 表格格式化器**反复回写（表格补空格填充、行尾硬换行、注入连续空行触发 MD012，并曾整段吞掉新章节）。提交前应核对索引快照（`git show :<path>`），提交后核对 `git show HEAD:<path>` 的行数与章节标记。

---

### 11.5 第三轮（2026-10-14）：20324 MSC4354 sticky expiry 已实现

**上游原文**（`changelog.d/20324.bugfix`，本轮实测拉取，非二手转述）：

> Fix the expiry of MSC4354 Sticky Events being reported differently to how the server tracks them.

**原判定回顾**（§10.2）：本仓 sticky 无 expiry 字段 ⇒ 判"无对应形态 / 特性缺口"。该描述正确，但缺口本身可以补 —— 本轮补上了。

**落点**：

| 项 | 位置 | 说明 |
| --- | --- | --- |
| schema | `migrations/00000000_unified_schema_v12.sql`（`room_sticky_events` 段 + 文件末尾幂等 ALTER） | 新增 `sticky_duration_ms` / `expires_at`；两列由 `ck_room_sticky_events_expiry_pair` 强制**成对**（避免"报了时长却永不失效"），另有 `ck_room_sticky_events_duration_positive` |
| 解析 | `synapse-storage/src/sticky_event.rs` 的 `resolve_sticky_expiry` | 同一个 `now_ms` 同时决定 `created_ts` 与 `expires_at`；上限 `MAX_STICKY_DURATION_MS = 3_600_000`（＝上游 `StickyEvent.MAX_DURATION`）；用 `saturating_add` 防时钟接近 i64 上限时回绕成负数（那会让置顶**永不过期**） |
| 读路径 | 同文件的 `get_is_sticky_event` / `get_all_is_sticky_events` / `get_rooms_with_is_sticky_events` | 统一谓词 `expires_at IS NULL OR expires_at > now`（严格大于 ⇒ 到达截止时刻即已失效） |
| 回报 | `synapse-web/src/routes/sticky_event.rs` | `GET` 响应新增 `sticky_expiry_ts`（直接读 `expires_at` 同一列）与 `sticky_duration_ms`；`POST` 接受提案拼写 `org.matrix.msc4354.sticky_duration_ms` 与短名 `sticky_duration_ms`，提案拼写优先 |
| 错误分类 | `synapse-storage` 的 `SetStickyEventError` + `synapse-services/src/room/service.rs` | 时长越界映射为 **400**（`invalid_param`），不再被上层包成 500 |
| 联邦 | `synapse-services/src/room/service.rs`、`synapse-web/src/federation/edu.rs` | EDU 透传 `sticky_duration_ms`；旧对等端缺该字段 ⇒ 回落"永不过期"，与改动前线上行为一致 |

**为什么"上报值＝追踪值"在本实现里是结构性的**：客户端拿到的 `sticky_expiry_ts` 就是读路径用于过滤的那一列 `expires_at`，两者**不存在**第二次计算，因此不可能像上游那样分叉。测试再钉一层 `expires_at == created_ts + sticky_duration_ms`，因为"哪一次时钟读取"仍是一个可写错的地方（下表变异 ② 就是这一类）。

**验证（全部实跑，命令可复制）**：

```bash
export DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
export TEST_DATABASE_URL=$DATABASE_URL

cargo nextest run -p synapse-storage --lib --features test-utils --locked -E 'test(sticky)'   # 19 passed
cargo nextest run -p synapse-web --lib --features test-utils --locked -E 'test(sticky)'       # 15 passed
cargo nextest run --test unit --features test-utils --locked \
  -E 'test(baseline_fingerprint_is_the_single_v12_source)'                                    # passed
```

基线指纹随 schema 变更同步更新：`5076e5daa3c35b1d` → **`5654afa2ea622aff`**（`tests/unit/test_isolation_unification_tests.rs` 的 `EXPECTED_BASELINE_FINGERPRINT`，用 FNV-1a64 独立复算过）。`.sqlx` 缓存经唯一入口 `scripts/ci/sqlx_prepare.sh` 重生成：**4 删 4 增**（正好是被改的 4 条查询，未缩容）。

**变异自证（红 → 还原 → 绿，两条都做）**：

| 变异 | 期望 | 实测 |
| --- | --- | --- |
| 把 `expires_at > $4` 改成恒真（`$4::bigint - $4::bigint + 1`） | `test_expired_pin_is_not_reported_on_any_read_path` 转红 | ✅ 红：`20324: an expired pin must not be reported` |
| 让 `expires_at` 用**另一次**时钟读取计算（`now + 1`），模拟 20324 的漂移形态 | `test_reported_expiry_is_the_tracked_expiry` 转红 | ✅ 红：`left 1791952745118 / right 1791952745117`（差 1ms 被抓） |

两条变异均从备份 `cp` 还原、`sha256sum` 比对一致、`grep -c MUTATION` 为 0。

> ⚠️ 变异 ① 改变了 `query_as!` 的 SQL 文本 ⇒ `SQLX_OFFLINE=true` 下会报 "no cached data"。
> 做法：变异期间改用在线模式 `SQLX_OFFLINE=false DATABASE_URL=<已迁移库>` 编译（本仓的
> `synapse_sqlx_scratch`），跑完再还原。**不要**为变异去重生成 `.sqlx`。

**残留差异（有意，非缺陷）**：

- 上游把 duration 作为 `/send`、`/state` 的 **query 参数**传入；本仓的 sticky 是**服务端置顶表**模型，时长从 `POST /rooms/{room_id}/sticky_events` 的请求体读取。这是两种不同的 MSC4354 落地形态，本文件 §10.2 已登记过该形态差异。
- 跨服务端到期时刻由各自时钟推算（偏差上界即 1h 时长上限），未做绝对时刻同步。

**顺带修复（既有红，非本批引入）**：`scripts/shell_routes_allowlist.txt` 里 `app_service.rs` 的 3 条登记行号（636/655/674）与 `synapse-web/src/routes/app_service.rs` 实测位置（646/665/684）**在 HEAD 上就已漂移 10 行**，导致 `placeholder_scan_tests::test_empty_json_successes_are_allowlisted` 在 HEAD 上本来就红（判据：`git diff --stat HEAD -- synapse-web/src/routes/app_service.rs` 为空，且该 allowlist 未被任何提交更新）。本批一并改正；同一 guard 里 `sticky_event.rs` 的两条因本次改动而位移（134/152 → 157/175）也已同步。

---

### 11.6 第三轮（2026-10-14）：`m.room.history_visibility` 强制执行（6 条读路径）

**原判定**（§11.3 第 4 条，由 §10.2 的 20295 判定顺带发现）：该值被存储、被回显、被当作鉴权事件保护，但**从未在任何读端点上强制执行** —— 任何当前成员都能翻到房间创建以来的全部历史，包括自己加入之前的一切。

**本轮落地**：新增 `synapse-services/src/room/messaging/history_visibility.rs`，把该值收敛为**一个**可测的判定。

| 值 | 谁可读 | 边界 |
| --- | --- | --- |
| `world_readable` | 任何人 | 不裁剪 |
| `shared` | 成员 | 不裁剪 |
| `invited` | 自被邀请起 | 首个 `invite`/`join` 成员事件的 `stream_ordering` |
| `joined` | 自加入起 | 首个 `join` 成员事件的 `stream_ordering` |

- **规则来源**：上游 `synapse/visibility.py::_check_membership`（本轮实测拉取原文核对）。上游按"**事件发生那一刻**的成员身份"判定；本仓没有为每个事件保留 state group，故把同一判定表达为**一条 stream-ordering 边界**（房间可见性值不变时二者等价）。
- **策略真源**：读**当前 `m.room.history_visibility` state event**，不读 `rooms.history_visibility` 投影 —— 投影只在建房与 summary 更新时写，客户端事后改可见性（power level 100 的普通状态事件）会被旧值误判。
- **边界查询**：新增 `EventReader::get_membership_boundary_stream_ordering`（实现落 `synapse-storage/src/event/history_boundary.rs`），对 `m.room.member` 取 `MIN(stream_ordering)`。`None`（从未被邀请/加入）⇒ **什么都看不到**，不是"全都能看"。
- **已接入的读路径**（6 条）：
  1. `GET /rooms/{roomId}/messages`（`MessagingService::get_room_messages`）—— 只过滤**返回页**，分页游标仍取自未过滤页，避免客户端在"加入前历史"上原地打转；
  2. `GET /rooms/{roomId}/event/{eventId}`（新增 `MessagingService::get_event_for_client`）—— 边界外的事件返回 **404** 而非 403，否则该端点会变成"这个事件存在但你无权看"的预言机；
  3. `GET /rooms/{roomId}/context/{eventId}` —— 目标事件走同一个 `get_event_for_client`；
  4. `GET /sync` 时间线（`sync_service/history_visibility.rs`，`d1b454d5c`）—— **曾经是唯一不需要客户端任何动作就会泄露的路径**：新成员首次（无 `since` 的）sync 会自动拿到加入前时间线。裁剪发生在 `apply_timeline_limit` **之前**，使 `limited` 描述"调用者可读的这一页"；
  5. `GET /rooms/{roomId}/relations/{eventId}/{relType}`（`RelationsService::get_relations`，本批）—— 裁剪在**派生分页游标之前**，游标因此只指向可见切片（否则"跳过一条被隐藏的 relation"会连带跳过一条可见的）。`get_relations` 的 `user_id` 是**必填参数**（不是 `Option`），即规则不可因"忘记传调用者"而被绕过。
  6. `POST /search`（`SearchService::search_room_events`）—— 存储查询把 `stream_ordering` 带出到 `SearchRoomEvent`（`#[serde(skip)]`，**不改变响应形状**），服务按页内出现过的房间**批量**求 cut 后裁剪；`next_batch` 仍由未过滤页导出。
- **连带修掉一个缓存缺陷**（由 `/search` 的测试暴露）：`/search` 的缓存键是 `(user, term, limit, next_batch)`，**不含可见性** ⇒ 一次 `shared → joined` 变更后，缓存会继续吐出**加入前的命中**直到条目过期（`m.room.history_visibility` 是普通状态事件，客户端随时可改）。修法：新增 `SearchService::has_restrictive_room`，**只要调用者参与的房间里有任何受限策略就完全绕过缓存**（读与写都绕），并把该调用**失败即视为受限**（fail-closed）—— 内部错误不能变成"静默地按无策略判定"。全 `shared` 的常见情形缓存照旧生效。
- **规则只有一份实现**：`room::messaging::history_visibility::{history_cut_with, history_cuts_for_rooms}` 是唯一实现，`/messages`、`/event`、`/context`（`MessagingService::history_cut`）、`/sync`（`SyncService::history_cuts`）、`/search` 都调它。读路径各写一份正是本缺口能长期存在的原因。

**仍未接入**：

| 端点 | 判定 |
| --- | --- |
| `/notifications` | **结构性不适用**（不需要接入）。`notifications` 表只由**显式推送**写入：生产调用链只有 `push/service.rs:341`（`PushNotificationService::send_notification`，由 `/_synapse/…` 推送端点触发），**事件管线不写这张表**（本仓与既有结论一致：没有服务端推送规则评估引擎）。⇒ 房间里发生的事件根本不会给"后加入的成员"产出一行 ⇒ 没有"加入前的通知"可供裁剪。**不发散地加过滤器**：加了只会造成"已覆盖"的错觉，并可能破坏"通知是发给某人的"这一语义。 |
| unread 计数（`/sync`、sliding sync） | ✅ **2026-10-14 第四轮已修**（并顺带发现同一轮的**第三条**泄漏路径）→ 见 **§11.10**。下表为该轮**修复前**的判定，保留以便对照：⚠️ **真实缺口**：`get_unread_counts`（`synapse-storage/src/event/unread.rs:25`）用 `read_markers` 的时间戳当窗口下界，**没有可见性下界**。新加入的成员在该房间**没有 read marker** ⇒ `last_read_ts = 0` ⇒ `notification_count` 会把**加入前**的非状态事件也数进去（计入条件是「非自己发送 + 非状态事件 + 时间戳晚于窗口下界」）。这是通过**计数**而非通过列表泄露的（较轻微，但确实是错的值）。**规则**：把窗口下界 clamp 到"该用户可见的最早事件"，即取 `GREATEST(read_marker_ts, 边界成员事件的 origin_server_ts)` —— 计数是时间戳口径，故需要边界事件的 `origin_server_ts`（现有的边界查询只回 `stream_ordering`，要一并带出）。**未做的原因**：属独立改动（改 `unread.rs` 的两个查询 + `get_unread_counts_batch` 的签名 + sync 的接线），且**不影响"列表是否泄露"**，故单列。 |

⇒ §11.6 初次落地时记的"**首次 `/sync` 会泄露加入前时间线**"这一条**已关闭**。

**有意保留的简化**（已写进模块文档）：

- 可见性值中途变化时按**当前值**回溯整条时间线（上游按每个事件当时的值）。方向是**更严**，不会更松。
- 曾"加入 → 离开 → 再加入"的用户保留最初一次的权限（上游对自身成员事件取"更 joined"的一侧，理由相同）。
- 未知/损坏的值回落到 `shared`（＝**fail-open**），与上游 `get_effective_room_visibility_from_state` 一致：该值来自受鉴权保护的状态事件，"读不出来"意味着"没有记录策略"，而不是"隐藏一切"。
- `/relations` **只裁剪关系事件本身，不校验被指向的 target 事件是否可见**。上游会连 target 一起判；本仓不判的原因是服务级测试大量使用**不存在的** target event id（`$orig_…`），加 target 校验会连测试语义一起改。target 本身的可见性由 `/event`（已接入）把关。

**验证（实跑）**：

```bash
export DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
export TEST_DATABASE_URL=$DATABASE_URL TEST_DB_TEMPLATE_SCHEMA=test_template_ci REDIS_URL=redis://localhost:6379
cargo nextest run --test integration --all-features --locked -E 'test(history_visibility)'
```

`tests/integration/api_history_visibility_tests.rs` 共 6 条真实路由测试：`joined` 隐藏加入前消息**且保留**加入后消息、创建者仍看得见全部（证明边界是**按调用者**的）、切回 `shared` 恢复历史；`invited` 下"从未被邀请者"仍受自己的 join 边界约束；单事件读对被隐藏事件返回 404；**`/sync`** 时间线同样隐藏加入前消息；**`/relations`** 对加入前发出的 annotation 同样隐藏；**`/search`** 对加入前写入的命中同样隐藏（这条同时钉住了上面的缓存缺陷：若缓存仍被使用，该用例在"先 `shared` 后 `joined`"的顺序下必然失败）。

**变异自证（红 → 还原 → 绿，五条）**：

| 变异 | 期望 | 实测 |
| --- | --- | --- |
| `/messages` 的过滤谓词恒真 | `joined_visibility_…` 转红 | ✅ 红：`joined` visibility must hide history from before the caller joined |
| `get_event_for_client` 的边界检查恒假 | `single_event_read_…` 转红 | ✅ 红（同批 3 条全红） |
| `/sync` 的接线换成 `HistoryCut::Unrestricted` | `sync_timeline_…` 转红 | ✅ 红：`joined` visibility must hide pre-join history from /sync as well |
| `/relations` 的过滤谓词恒真 | `relations_hide_annotations_…` 转红 | ✅ 红：`joined` must hide annotations made before the caller joined |
| `/search` 的过滤谓词恒真 | `search_hides_hits_…` 转红 | ✅ 红：`joined` must hide search hits from before the caller joined |

五条均从备份 `cp` 还原、`sha256sum` 比对一致、`grep -c MUTATION` 为 0。

**`/sync` 的实现要点（性能）**：`SyncService::history_cuts` 是**批量**的 —— 一次 sync 覆盖所有 joined 房间，逐房间照搬单房间规则要 2N 次查询；实现改为 ① 一次 `get_state_events_by_type_batch` 取全部房间的可见性值，② 只对 `invited`/`joined` 的房间再做边界查询。`shared` 是房间版本默认值，故绝大多数 sync 的第二步**执行 0 次**。它被并进既有的 `tokio::try_join!`（与其余房间数据同一轮往返，不增加串行等待）。

**⚠️ 本批顺带修正的真实缺口**：`tests/integration` 目标带
`required-features = ["test-utils", "privacy-ext", "voice-extended", "voip-tracking", "beacons", "server-notifications"]`
⇒ **`cargo clippy --features test-utils` 编译不到这个目标**（Cargo 直接跳过），只有 `--all-features` 才编译。
20324 批次因此漏改了 `tests/integration/api_sticky_event_tests.rs`（5 参调用 + 旧结构体字面量），
本批一并修复。**教训**：本仓的"工作区 clippy"必须带 `--all-features`，否则整个 integration 目标被静默跳过。

---

### 11.7 19354（MSC4140 专用调度端点）—— **2026-10-14 已实施**

> **本节结构**：先留档**原方案**（用于对照"计划 vs 实际"），再给**实现记录**
> —— 含两处与方案**不同的决定**及其判据、契约链 9 步的实际执行记录、验证与残留差异。

**原方案的出发点（当时不做）**：这是唯一仍未对齐的**产品**面，但它是一条**路由面**变更 —— 真实成本不是 handler（约 80 行），
而是整条契约链（3 张 `derived_route_table_*.inc.rs` + 6 份 fixtures + 2 份 `route_ledger_*.snapshot` +
`docs/openapi/route-table.json` + `ROUTE_CONTRACT.md` + 对比文档计数 + `ledger_annotations.txt`）。
只做一半，正是本文件 §「生成物漏刷」记录过的**最高频既有红形态**。故当时把方案写死，留给独立批次。

**handler 设计（可直接照做）**

- 路径：`PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}`
  —— 加到 `create_delayed_events_router()` 即可，它已经被 `assembly.rs:257` nest 在
  `/_matrix/client/unstable/org.matrix.msc4140` 之下，拼出来就是上游的同一条路径。
- 签名：`State<AdminContext>` + `HeaderMap` + `AuthenticatedUser` + `Path<(RoomId, String, String)>` + `Json<Value>`；
  先 `ensure_room_member_admin(...)`（`AdminContext` 版本，见 `room_access.rs:77`），再 `normalize_room_event_type(&event_type)`。
- 请求体：`delay_ms` 必填正整数且 ≤ `86_400_000`；`content` 必填对象；`state_key` 可选字符串；
  `org.matrix.msc4354.sticky_duration_ms` 可选整数（见下）。
- 响应：`{"delay_id": <数字>}` —— 与上游的**字符串**形态**有意不同**：本仓 `delay_id` 是 `BIGSERIAL` i64，
  同族的 `GET`/`POST /delayed_events/{delay_id}` 用 `Path<i64>` 解析，返回字符串会让本仓自己的管理端点无法消费。
- 幂等：缓存键 `msc4140-delayed-event:{user}:{room}:{txn}` → `{"delay_id": N}`（ttl 1h）。
  **前缀必须与 `/send` 的 `txn:` 区分**：那个命名空间存的是**立即发送**的 event_id，共用会让同 txn 的普通 `/send` 顶掉调度调用。
- **`sticky_duration_ms` 本批不支持**：`delayed_events` 表没有该列，能诚实的只有两条路 ——
  ① 加列并在 dispatcher 真正发出事件时调用 `set_is_sticky_event(...)`（会连带第二次 schema 变更：
  基线指纹 + `.sqlx`，值得单独一个 commit）；② 明确返回 501。
  **唯独不能"收下但忽略"** —— 那是静默丢弃客户端要求的行为，正是 20324 想消灭的那类不一致。

**契约链重生成清单（顺序不可反，每步都有门禁）**

1. `scripts/contract/ledger_annotations.txt` **同时**改 `extract_registered.py` 的 `EXPECTED_ANNOTATIONS`
   （两者须逐键相等）；新行：`PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}` + `auth=user`
   （提取器推不出 auth，照抄同族两条 MSC4140 端点的写法）。
2. `python3 scripts/contract/gen_derived_routes.py --bootstrap`（fixture 与源码面同时变化时会死锁在 `verify_fixtures()`）。
3. 重导 fixtures：`cargo build --bin synapse_ledger_export`（**default features**）
   → `--profile={default,worker,all}` × 两个 lane（sdk lane 另加 `--features all-extensions`），
   钉 `--timestamp=2026-05-02T00:00:00Z --commit=000…0`。
4. `tests/integration/snapshots/route_ledger_{default,worker_enabled}.snapshot`（**最常漏**）。
5. `python3 scripts/contract/gen_contract_doc.py` → `docs/synapse-rust/ROUTE_CONTRACT.md`。
6. `docs/openapi/route-table.json`：输入必须是 **CI 口径**（default-feature 新鲜导出 + `FIXED_TIMESTAMP`）。
7. `docs/synapse-rust-vs-synapse-comparison.md` 的 `N 条注册路由` 计数（守卫
   `doc_credibility_guard_tests::counts_match_the_route_contract`）。
8. 最后手动 `bash scripts/contract/check_route_contract.sh` —— **preflight 在改动集触及契约链文件时会 SKIP 契约 gate，exit 0 不等于契约链绿**。
9. 决定是否把新路径加入 `DELAYED_EVENTS_WORKER_PATHS`（Guard B 用 `path.contains("delayed_events")`，
   而新路径是**单数** `delayed_event` ⇒ 不加不会红，但语义上同族；建议加入）。

#### 11.7.1 实现记录（2026-10-14）

**与方案的两处偏离（都是必要的，且第 1 处是真实的提权）**

1. **写鉴权不能只判成员** —— 方案原稿写的是 `ensure_room_member_admin(...)`。

   取证：`src/server/mod.rs:841-871` 的 dispatcher 在到期派发时**不做任何再鉴权** ——
   状态事件走 `room_service.messaging().create_event(...)`、消息走
   `send_message_with_txn(...)`，两者都不查权限 ⇒ **排期时的检查是唯一的闸门**。
   若只判成员，普通成员可以排一个 `m.room.power_levels`（`state_key: ""`），
   由后台任务落库 —— **提权，且没有调用者可归因**。

   改法：按 `state_key` 有无分别走 `ensure_room_state_write_access` 与新增的
   `ensure_message_write_access`（＝`/send` 的**整套**前置门：power level / 房间加密 /
   PL 变更规则 / MSC3806 内容扫描）。两者住 `handlers/room/write_access.rs`（从 `mod.rs` 拆出，见下），
   `/send` 改为调用同一个函数 ⇒ 两扇门不可能再各自漂移。

2. **`sticky_duration_ms` 取方案 ②：明确 501**，不取 ①（给 `delayed_events` 加列 + dispatcher 落地）。
   理由（本仓模型使然）：本仓的 sticky 是 **per-`(room, user, event_type)` 的置顶槽位**
   （`room_sticky_events` + `POST /rooms/{roomId}/sticky_events`），**不是事件的属性**；
   "排期并在到期时自动置顶"会**静默覆盖**用户在同档位上已有的置顶（上游没有这个冲突，
   因为它的 sticky 是事件属性）。且 ① 要连带第二次 schema 变更（基线指纹 + `.sqlx`）。
   ⇒ 新增 `ApiError::not_supported()`（**501** + `M_UNKNOWN`），并把该 `(kind, code)` 分歧登记进
   `STATUS_DIVERGENCE_WHITELIST`（Matrix 未为 501 定义 errcode）与
   `all_fixed_api_error_constructors()`。**两种拼写都拒**
   （`org.matrix.msc4354.sticky_duration_ms` 与短名 `sticky_duration_ms`），因为置顶端点两种都收。

**落点**

| 文件 | 改动 |
| --- | --- |
| `synapse-web/src/routes/delayed_events.rs` | 新增 `schedule_delayed_event`（PUT）＋纯函数 `parse_schedule_request`（校验矩阵，可离线单测）＋路由挂载 |
| `synapse-web/src/routes/handlers/room/write_access.rs` | **新文件（120 行）**：`ensure_room_state_write_access`（自 `mod.rs` 迁入）＋新增 `ensure_message_write_access` |
| `synapse-web/src/routes/handlers/room/mod.rs` | 拆出上者后 **501 → 403** 行；新增 `MSC4140_MAX_DELAY_MS`、`MSC4140_{SEND,SCHEDULE}_TXN_NAMESPACE`、`schedule_delayed_event_core`（两条排期面共用的存储＋幂等尾段） |
| `synapse-web/src/routes/handlers/room/events.rs` | `send_message` 的前置门改为调用 `ensure_message_write_access`；**1456 → 1416** 行 |
| `synapse-common/src/error.rs` | 新增 `ApiError::not_supported()` ＋分歧登记 ＋构造器清单 |
| `synapse-services/src/worker/topology_validator.rs` | `DELAYED_EVENTS_WORKER_PATHS` 纳入新路径（见第 9 步） |
| `synapse-web/src/routes/worker.rs` | 该集合的响应断言随之更新 |
| `tests/integration/api_delayed_event_schedule_tests.rs` | **新文件**：7 条真实路由用例 |
| `tests/unit/worker_delayed_events_ownership_tests.rs` | Guard B 过滤词由复数收紧为单数词干（见第 9 步） |

**⚠️ 第 9 步的真实发现：Guard B 的过滤词**结构上**漏了这条路径**

方案已预判"不加也不会红"，但根因比预判更值得记住：Guard B 用的是
`path.contains("delayed_events")`（**复数**），而新路径是**单数** `delayed_event`
⇒ 这条路径**不可能被该守卫发现**，不是"恰好没加进集合"。
故把过滤词收紧为单数词干 `delayed_event`（同时覆盖两种拼写），并补一条"族内至少 2 条"的非空性断言；
`delayed_events_gate_rejects_non_family_paths` 也补了一个单数形态的反例（少一段）。

**⚠️ 棘轮（方案清单之外、实测撞到的教训）**

把两个 helper 直接写进 `handlers/room/mod.rs` 后该文件变成 **501 行** ⇒ 越过 500 阈值、
成为**新的超限文件**，把棘轮计数从 257 推到 **258**，`preflight` 因此在提交时**拦下了这次提交**。
故拆出 `write_access.rs`，`mod.rs` 回到 **403 行**（`events.rs` 因前置门外移同时 1456 → 1416）。
**改本仓任何"接近 500 行"的文件前，先跑一次棘轮。**

**契约链 9 步的实际执行记录**

| 步 | 命令 | 结果 |
| --- | --- | --- |
| 1 | `ledger_annotations.txt` ＋ `EXPECTED_ANNOTATIONS` 同批加 `PUT … + auth=user` | `check_annotation_fidelity` 未报差异 |
| 2 | `gen_derived_routes.py --bootstrap` | 6/6 lane 报 `extra=[('PUT', '…/delayed_event/…')]`（预期红），三张表按预期重写 |
| 3 | `generate_sdk_ledger_fixtures.sh` ＋ 三次 `synapse_ledger_export`（default features，钉 `2026-05-02T00:00:00Z` / 全 0 commit） | 7 个文件更新（6 份 JSON ＋ SDK `manifest.json`） |
| 4 | `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1 … -E 'test(api_route_ledger)'` | 15/15 通过，两份快照重生成 |
| 5 | `gen_contract_doc.py` | `ROUTE_CONTRACT.md`：**1028** 条 / 65 模块（延迟事件族 2 → 3） |
| 6 | `gen_route_table.py --ledger tests/unit/fixtures/ledger_export/default.json` | `route-table.json` **948 → 949**；`lane=golden` / `profile=default` / `generated_at=2026-05-02T00:00:00Z` |
| 7 | 比较文档 3 处计数 ＋ `docs/openapi/README.md` 与对账脚本注释 | `1,027 → 1,028` 条注册路由；增量已接在历史台账后 |
| 8 | `bash scripts/contract/check_route_contract.sh` | 全绿（派生表去重 1030 → 1028、6 坐标计数、`registered_by` 保真、cfg 门控、SDK⊆ledger 均通过） |
| 9 | `DELAYED_EVENTS_WORKER_PATHS` ＋ Guard B 收紧 | 见上 |

**验证（实跑）**

```bash
export DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
export TEST_DATABASE_URL=$DATABASE_URL TEST_DB_TEMPLATE_SCHEMA=test_template_ci REDIS_URL=redis://localhost:6379

cargo nextest run -p synapse-web --lib --features test-utils --locked \
  -E 'test(delayed_event) + test(schedule_body)'                       # 8 passed
cargo nextest run --test integration --all-features --locked \
  -E 'test(delayed_event_schedule)'                                    # 7 passed
```

纯函数单测覆盖整个**校验/拒收矩阵**（无 DB、无房间）：`delay_ms` 缺失/非整数/浮点/`null`、
`0`/负数/超上限（含**边界含入** `86_400_000` 与超上限 1ms）；`content` 缺失/数组/字符串；
`state_key` 缺省/显式 `null`/空串/命名/数字；`sticky_duration_ms` 两种拼写、
**即使值为 `null` 也拒**（证明该分支拒的是字段而非某个取值）。

真实路由用例（`tests/integration/api_delayed_event_schedule_tests.rs`，7 条）：

| 用例 | 钉住什么 |
| --- | --- |
| `dedicated_endpoint_schedules_a_message_event` | 返回的 `delay_id` 能被同族管理端点读回；库内恰 1 行 |
| `dedicated_endpoint_carries_the_state_key_into_the_stored_row` | `state_key` 落库（决定 dispatcher 走状态路径）＋**对照**：无 `state_key` 时响应里不得出现该字段 |
| `dedicated_endpoint_replays_the_same_delay_id_for_the_same_txn_id` | 同 txn 重放 ⇒ 同一 `delay_id` 且**不新增行** |
| `dedicated_endpoint_and_send_keep_separate_idempotency_namespaces` | 两个方向都钉：先排期后普通 `/send` 同 txn ⇒ 得 `event_id` 而非 `delay_id`；先 `/send` 后排期 ⇒ 得 `delay_id` 而非 `event_id` |
| `dedicated_endpoint_refuses_a_state_event_the_caller_may_not_write` | **反提权**：PL 0 成员排 `m.room.power_levels` ⇒ 403；两条对照（同成员可排消息、创建者可排同一状态事件） |
| `dedicated_endpoint_refuses_non_members_and_unknown_rooms` | 非成员 403、未知房间 404 ＋ 对照（同 body 由成员发 ⇒ OK） |
| `dedicated_endpoint_refuses_the_sticky_duration_hint` | **501 ＋ `M_UNKNOWN`** ＋ 对照（去掉该字段即 200） |

**变异自证（红 → 还原 → 绿）**

| 变异 | 期望 | 实测 |
| --- | --- | --- |
| A：把状态分支换回**方案原稿的"只判成员"**（`ensure_room_member_ctx` 取代 `ensure_room_state_write_access`） | `…refuses_a_state_event_the_caller_may_not_write` 转红 | ✅ **红**：`left: 200 / right: 403`，响应体 `{"delay_id":1}` —— 即"PL 0 的成员成功排下了 power_levels 写"，提权被实证 |
| B1：Guard B 用**收紧后的单数词干**，且集合里删掉新路径 | `registered_delayed_events_routes_are_all_listed` 转红 | ✅ **红**：`Guard B failed: registered delayed_events route "…/delayed_event/{event_type}/{txn_id}" is missing from DELAYED_EVENTS_WORKER_PATHS` |
| B2：过滤词退回**复数** `delayed_events`（集合仍缺该条目）—— 证"旧守卫结构上看不见它" | 该用例**绿**（＝洞） | ✅ **绿**：`1 passed`。与 B1 成对，即"旧守卫会放行、新守卫能抓住" |

三条均从备份 `cp` 还原、`sha256sum` 与变异前**逐字节一致**、
`grep -rn MUTATION synapse-web/src tests/unit/... synapse-services/...` 为 0。

**残留差异（有意，属独立待办）**

- 上游的三条管理子路径 `POST .../delayed_events/{delay_id}/{cancel,send,restart}` 与列表端点
  `GET .../delayed_events` 仍未提供；功能已由既有 `POST .../{delay_id}` 的 `action` 覆盖 ⇒
  属**形态差异**，不是能力缺失。
- `/send`、`/state` 的 **query 参数**路径仍会**静默忽略** `org.matrix.msc4354.sticky_duration_ms`
  （本轮只在新端点收紧为 501）。上游把该字段读作 query 参数放在这两条上，故这是一处
  **尚存的静默丢弃**：要么在共享函数里对 query 形态也 501，要么等 MSC4354 形态稳定后一起做。
  记录于此，以免被"新端点已经拒了"顺带盖过去。

### 11.8 本轮复核中发现、**与本方案无关**的既有红（勿误判为回归）

CI 在**本批改动之前**的 HEAD（`cc56d479e`，run 38509545932）已有 3 个 job 失败：

| 失败 job | 根因（CI 日志原文） |
| --- | --- |
| `Repo Sanity` | `file_size: 779 files scanned, 257 over 500 (baseline 253)` —— `scripts/ci/check_file_size_ratchet.py` 被顶红（`capability_governance.rs: 1016 -> 1197`、`application_service/*`、`sync_service/*`） |
| `Test & Lint (*)` | `check_dashboard_metrics.py` 新增 1 条 No-data 面板指标 `synapse_storage_stream_current_position`（`storage-performance.json`） |
| `OpenAPI Artifact` | `client.yaml + route-table.json` 与源码不一致 |

**⚠️ 该 CI 日志本身不能用来断言"integration 车道是绿的"**：最近一次失败的
`Test & Lint` 在更早的步骤（`check_dashboard_metrics.py`）中止，**integration 步骤根本没有执行**
（该 job 的日志里 `api_route_ledger_tests` 零命中）。这一条在 2026-10-14 变成了实锤 ——
见下面的 `route_ledger_*.snapshot` 条目。

本地另有 5 类（属环境伪影、既有漂移或待定性）：

- `ci_test_scope_tests::*` 两条：用例内部 `Command::new("bash")` 跑 grep，子进程落到 CLI 前置的 toybox shim；
  **实证**：同一条 `grep -rnE '\bunsafe…'` toybox 得 0 行、`/usr/bin/grep` 得 2 行 ⇒ 在 HEAD 上本来就红。
- `sqlx_ratio_gate_tests::*` 4 条：file-broker 拦 `synapse-services/src/builtin_oidc_provider.rs` 导致 census 子进程崩；
  `PYTHONPATH=<pyshim> SYNAPSE_BROKER_ROOT=$PWD` 复跑即绿。
- `sqlx_dynamic_literal_guard_tests::no_new_production_literal_dynamic_sql`：报
  `synapse-e2ee/src/megolm/storage/db_tests.rs` 6 处"生产区"字面量，而该文件**未被本批改动**，
  且 `sqlx_query_census.py`（内部无子进程）独立复现同一误判 ⇒ 疑似 `#[cfg(test)] mod x;` 写在
  **非 `mod.rs`** 文件里时模块路径解析有误。**未改**：修它要同时下调棘轮基线，应单独批次。
- `api_route_snapshots_tests::{snapshot_versions_endpoint, snapshot_capabilities_v3}`：
  差异为 `org.matrix.msc3245: true → false` 与 `m.room.summary.enabled: true → false`。
  两者由**路由面**推导（`capability_governance.rs::msc3245_capability` / `room_summary_capability`
  → `manifest_has_route("GET", "/_matrix/client/v3/rooms/{room_id}/summary")`），而该路由确实存在于
  `derived_route_table_always.inc.rs`（17 处命中），`collect_route_surface()`（`handlers/versions.rs:52`）
  取自 `assembly::declared_ledger_all()` ⇒ 待查该函数为何不吐这条路由。**未更新快照** ——
  更新等于把"能力被少报"钉成契约；应先查清 `declared_ledger_all()`。
  归属证据：本批改动未触及任何 versions / capabilities / 派生表文件。
- `api_route_ledger_tests::{declared_route_manifest_full_snapshot_matches_default_state,
  ..._matches_worker_enabled_state}` —— **既有漂移，2026-10-14 已单独提交修复**
  （提交标题 `chore(contract): 修正 route_ledger 集成快照的既有漂移（--all-features 口径）`）。
  两条 `route_ledger_*.snapshot` 的 `count` 停在 **980 / 991**、不含任何
  `[saml]` / `[cas]` 行，而 CI 与本地权威命令都是 `--all-features`，该口径下实际是 **1008 / 1019**
  ⇒ 快照比实际**少 28 条**，这两条用例在 `--all-features` 下**本来就红**。
  **判据（可复现、零副作用）**：把两条快照 `git checkout --` 回 HEAD 版，跑
  `SQLX_OFFLINE=true cargo nextest run --test integration --all-features -E 'test(declared_route_manifest_full_snapshot)'`
  ⇒ `2 failed`，左值 `count: 1009`（含紧随其后 19354 提交新增的 1 条）/ 右值 `count: 980`。
  修复单独成一个提交，**不裹进 19354**，理由是两者动机不同、且前者是"接受既有状态"的批量对齐。

**其中 file_size 棘轮对本仓有直接约束**：`synapse-storage/src/event/state.rs` 原为 **487 行**
（阈值 500，且不在 `scripts/ci/file_size_baseline` 里）⇒ 往里加任何查询都会让它成为**新的超限文件**，
把计数从 257 推到 258。本批因此把历史可见性边界查询放进新模块
`synapse-storage/src/event/history_boundary.rs`（51 行），`state.rs` 回到 487 行，棘轮维持 `257 (baseline 253)`。
**改本仓任何"已经接近 500 行"的文件前，先跑一次棘轮脚本比对数字。**

**2026-10-14 第五轮更新（→ §11.11）**：

- 本节列的 CI 红里，**供应链那条已转绿**：`RUSTSEC-2024-0449`（`unsafe-libyaml`，经 `serde_yaml 0.9`）
  已通过迁移 `serde_norway` **消除**，并顺带关掉 4 条当日新发布的 rustls 漏洞
  ⇒ `cargo deny check advisories` = `advisories ok`、`cargo audit` EXIT=0（§11.11.1 / §11.11.2）。
- 本节末尾那条 file_size 约束本轮**仍然遵守**：新增的 DB 守卫放进新文件
  `synapse-e2ee/src/key_rotation/service/db_tests.rs`，`key_rotation/service.rs` 只做小幅增删
  ⇒ 棘轮仍是 `257 over 500 (baseline 253)`，**未新增超限文件**。
- 本节未列的"既有红"里，`M-01`/`M-02`/`N-01`/`L-02`（E2EE v2.0 计划）经重判**已修**，
  另有两项**新发现只登记不改**（§11.11.3 / §11.11.5）。

---

### 11.9 E5（本文件被"陈旧副本"覆盖）：排查结论与处置

**现象**（2026-10-14 实测 2 次；此前累计 ≥4 次）：本文件在工作区被替换成一份**更旧、且表格被重新补空**的副本 —— `git status` 显示该文件「已修改」，diff 规模 **188 增 / 270 删**，`^### 11\.` 由 8 条掉到 6 条（§11.7/§11.8 **整段消失**）。因为格式依旧合法，**markdownlint / aspell 全绿** ⇒ 损伤是静默的。

**已排除的假设（逐条给出判据，均可复现）**

| 假设 | 判据 | 结论 |
| --- | --- | --- |
| 另一个 WorkBuddy 会话持陈旧缓冲回写 | 扫 `~/.workbuddy/traces/*/trace_*.json` 中含本文件路径的 **18 个 trace，`sessionId` 全部是本会话** `53602ade`；同级会话 `ee2a6cf3`（cwd `~/Desktop/hu_ts`）对本文件**零写入** | **否证** |
| 仓库工具链重排表格后回写 | 全仓无任何脚本写 `docs/audit/*.md`；`.pre-commit-config.yaml` 只有 trailing-whitespace / end-of-file / mixed-line-ending / check-json / pretty-format-json / check-toml / check-yaml / cargo-fmt / ruff-format / shfmt —— **没有任何 markdown 格式化器**；`preflight.sh` 对 md 只跑 markdownlint + aspell（只读） | **否证** |
| `Edit` 工具基于陈旧读缓存做读-改-写 | 对照实验：先用 `printf` 从 bash 追加哨兵（`Edit` 不知情），再用 `Edit` 改首行标题 ⇒ **哨兵保留、章节仍 8 条、diff 恰好 3 行** | **否证** |
| `git stash` 应用了旧内容 | `git stash list` 只有一条 `WIP on feature/2026-10-01-…`，其树里**没有**本文件 | **否证** |

**剩下的事实（可复现）**

- 工作区那份**不等于任何一个历史修订**：`git diff 6ef02fa1e 4504bc5bd -- <本文件>` 只有 **2 增 / 82 删**，而现场是 **188/270**。
- 它 = **更旧的内容 + 表格重新补空**。而"补空表格"是**本仓常态风格**：HEAD 版本文件自身就有 **9 处**补空分隔行（`^\| -{10,} \|`），另有 3 个文档同样如此；补空在本文件中自 `d63337805`（08:01）起出现。
- ⇒ 写入者产出的是**"读得更早、写得更晚"、且顺手重排表格**的副本 —— 这是**文档重写者**（LLM 会话 / 编辑器侧保存）的指纹，不是 formatter、不是 git、也不是另一个 agent 会话。

**处置（本轮落地）**

1. **章节棘轮**：新增 `scripts/ci/check_plan_doc_sections.py` + 基线 `scripts/ci/plan_doc_sections_baseline`（43 条标题）。
   基线里的标题在文档里找不到 ⇒ **判红并指名缺失章节**；新增/重排不红；`--update` 供有意删改（纪律同 `file_size_baseline`）。
   已接入 `scripts/quality/preflight.sh`（仅当本文件进入改动集时运行）。变异自证：对副本删掉 §11.8 后
   `--doc /tmp/plan_doc_mutated.md` **判红**并打印 `✗ ### 11.8 …`。
2. **恢复规程**：`git checkout -- docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md`。
   **本文件内容以 git 为准**；工作区版本可疑时先还原，再核 `grep -c '^### 11\.'`。
3. **提交纪律**：文档改动放最后、改完立刻提交；提交后必须 `git status` 干净。

**仍未根除的部分**：写入者不在仓库内，仓库侧**无法阻止**它再次发生 —— 棘轮只保证下次**立刻判红**而非静默。
若再次出现，唯一还有用的一手信息是"**发生时刻 + 当时开着的编辑器 / 预览面板**"：这是尚未排除的一侧。

**第四次实测（2026-10-14 19:53:17）—— 这次拿到了时间戳**：提交 `0fb7fa8e0` 于 **19:40:52** 落地后，
工作区在本轮第四次被替换成**第三轮之前**的副本（`### 11.6` 回到"**5 条读路径**"、`### 11.7` 回到
"**本轮未实施**"、**§11.9 整段消失**、§11.7.1 消失），`git diff --numstat` = **239 增 / 417 删**（净 −178 行），
文件 mtime = **19:53:17**，即**提交后 12 分 25 秒**。本次运行期间本会话**没有任何**对该文件的写入
（期间只跑编译/测试）⇒ 再次坐实"写入者不在本会话、也不在仓库工具链内"这一结论。
处置同前：`git checkout -- <file>` 从 HEAD 还原（9 个章节、§11.7.1/§11.9 复位），随后再改、再核。

**第五次实测（2026-10-14 21:03:38）—— 这次拿到了写入者的读窗口**：工作区再次被换成"第三轮之前"的副本
（`### 11.6` 回到"5 条读路径"、`### 11.7` 回到"本轮未实施"、**§11.9 与 §11.10 整段消失**），
`git diff --numstat` = **238 增 / 516 删**，mtime **21:03:38**。新证据：这份副本的**标题里带着 `EXPERIMENT` 后缀**，
而那正是本会话在**上一轮做对照实验时、只存在了一两分钟的哨兵**（见上"对照实验"）⇒ 写入者的缓冲区是在
**那个极短窗口内**读取的文件。据此用该哨兵串（含 JSON 转义形态）在 `~/.workbuddy`、
`~/Library/Application Support`、`~/Library/Caches` 全量搜索，**零命中** ⇒ 宿主不是这些目录里的明文文件。
处置同前：`git checkout -- <file>` 从 HEAD 还原（10 章节 / 45 条标题在位）。

---

### 11.10 第四轮（2026-10-14）：unread 计数的可见性下界 + `/rooms/{roomId}/sync` 的时间线

本轮把 §11.6 表里唯一还开着的可见性缺口关掉，并**顺带发现同一类的第三条泄漏路径**。

#### 一、先测量，再动手（"缺陷为真"的判据）

新增 `tests/integration/api_history_visibility_tests.rs::unread_counts_ignore_history_from_before_the_caller_joined`：
`joined` 房间 → alice 发 3 条 → bob 加入 → alice 再发 1 条 → 读 bob 的
`rooms.join[<room>].unread_notifications`。**修复前实测**（非推测）：

```text
assertion `left == right` failed: only the readable (post-join) message may be counted …
  left: 4
 right: 1
```

即 bob 的**时间线只有 1 条**（3 条加入前消息被所有列表面隐藏），而**计数是 4** ——
历史通过**计数**泄露。根因与 §11.6 的推断一致：加入**不写** read marker
（唯一写入点是客户端 receipt 路径 `set_read_marker`），故 `last_read_ts` 落到 `0`。

#### 二、规则与实现

**规则**（与 §11.6 预告一致）：窗口下界取 `GREATEST(read_marker_ts, 边界成员事件的 origin_server_ts)`。

| 落点 | 改动 |
| --- | --- |
| `synapse-storage/src/event/history_boundary.rs` | 新增 `MembershipBoundary { stream_ordering, origin_server_ts }`；边界查询由 `MIN(stream_ordering)` 改为 `ORDER BY stream_ordering LIMIT 1` —— **两个坐标必须来自同一行**，否则列表面与计数面会各自解出一个成员事件（先 invite 后 join）而互相矛盾 |
| `synapse-storage/src/event/reader.rs` | `get_membership_boundary_stream_ordering` → `get_membership_boundary`（`Option<MembershipBoundary>`），trait / 实现 / 内存 mock 同步 |
| `synapse-services/src/room/messaging/history_visibility.rs` | `HistoryCut::From` 携带两个坐标；新增 `unread_floor_ts()`：`Unrestricted → 0`（read marker 继续说话）、`From → 边界 ts`、`NOTHING → i64::MAX`（never-entitled ⇒ 计数为 0） |
| `synapse-storage/src/event/unread.rs` | 两个查询各加一个下界参数，SQL 只需把 `COALESCE(...)` 包一层 `GREATEST(..., $n)`；批量版入参改为 `&[(room_id, floor_ts)]` —— **成对而非两个平行数组**：错位会把 A 房的下界安到 B 房上，两个方向都是计数型泄漏 |
| `synapse-services/src/sync_service/{data_fetch,response,mod}.rs` | `/sync` 把 `history_cuts` 提到 `try_join!` **之前**，并把它一并交给 unread 批量（该值现在喂两个消费者）；`room_unread_counts` 单房间路径同样先解析 cut |

**为什么把 `history_cuts` 提出 `try_join!`**：计数用的下界必须与时间线过滤用的是**同一次**解析结果；
留在并发 join 里，两者可能各自解出一个边界。

#### 三、顺带发现：`/rooms/{roomId}/sync` 从未裁剪时间线（第三条同类泄漏）

`GET /_matrix/vendor/v1/rooms/{roomId}/sync`（**本仓自有客户端端点**；路由真伪只问派生表：
`derived_route_table_always.inc.rs` 中 `registered_by = "room"`）走
`SyncService::room_sync` → `build_room_sync`，而 `build_room_sync` **从不调用 `apply_history_cut`**
—— §11.6 的整轮收口漏掉了这个时间线装配点 ⇒ 新成员可从该端点拿到加入前时间线（与最初 `/sync` 那个缺口同级）。

**修法**：把 cut 做成 `BuildRoomSyncRequest` 的**必填字段**，由 `build_room_sync` 自己裁剪
（裁剪发生在 `apply_timeline_limit` **之前**，使 `limited` 描述"调用者可读的这一页"），
且**同一个 cut** 同时供给计数下界 ⇒ **一次解析、两个轴，调用方无法只做一半**。

#### 四、验证（实跑）

```bash
export DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
export TEST_DATABASE_URL=$DATABASE_URL TEST_DB_TEMPLATE_SCHEMA=test_template_ci REDIS_URL=redis://localhost:6379
cargo nextest run --test integration --all-features --locked -E 'test(history_visibility)'   # 9 passed
```

| 用例 | 钉住什么 |
| --- | --- |
| `unread_counts_ignore_history_from_before_the_caller_joined` | 计数与列表用**同一套**边界（修复前红：4 vs 1） |
| `unread_counts_still_count_history_a_shared_room_makes_readable` | **反向对照**：`shared` 房间里加入前消息**本来就可读**，必须照旧计 3 ⇒ 下界是**策略**下界，不是"按成员"下界（否则就是把一种 bug 换成另一种：静默少计） |
| `room_sync_hides_history_from_before_the_caller_joined` | 单房间 sync 也隐藏加入前消息，且创建者仍看全（按调用者） |

单测：`the_unread_floor_is_the_boundary_timestamp_or_zero`（三个分支）+ 既有 `HistoryCut` 用例改用结构体变体。

**变异自证（红 → 还原 → 绿，两条）**

| 变异 | 期望 | 实测 |
| --- | --- | --- |
| A：批量 SQL 里让 floor 参与但不生效（`tr.floor_ts - tr.floor_ts`；**保留参数引用**以免 PG 报"无法确定 $3 的类型"） | `unread_counts_ignore_history_…` 转红 | ✅ 红：`left: 4 / right: 1` —— 正是修复前的值 |
| B：去掉 `build_room_sync` 的 `apply_history_cut` | `room_sync_hides_history_…` 转红 | ✅ 红：`joined` must hide the pre-join message on /rooms/{roomId}/sync too |

两条均从备份 `cp` 还原、`sha256sum` 与变异前**逐字节一致**、`grep -rn MUTATION` 为 0。

**`.sqlx`**：SQL 文本变更经唯一入口 `scripts/ci/sqlx_prepare.sh` 重生成 —— **3 删 3 增**
（两条 unread + 边界查询），条数 **1318 → 1318（未缩容）**，无连带改动。

**⚠️ 本地跑变异 A 的两个坑（本轮实测）**：
① `SQLX_OFFLINE=false` 时 `DATABASE_URL` **必须是已迁移的库**：指向 `synapse_test`（`public` schema 未迁移）
会得到 **1593 个 `cannot infer type`** 编译错误，看起来像"到处都坏了"；正确姿势是**编译期用
`synapse_sqlx_scratch`、运行期用 `TEST_DATABASE_URL=synapse_test`**。
② 变异**改了 SQL 文本** ⇒ OFFLINE 模式报 "no cached data"，故变异期间必须走在线模式，跑完还原（**不要**为此重生成 `.sqlx`）。

**遗留（仍开着）**：

- unread 的 floor 只对**受限**可见性生效（`shared`/`world_readable` 恒为 0）—— 已有反向对照用例，
  但 `world_readable` 房间"非成员是否应有未读"未与上游逐条比对。
- `/rooms/{roomId}/sync` 的时间线现已裁剪，但其 `state` 段仍返回**当前**状态（与 `/sync` 一致），
  未按"事件发生时刻"回溯 —— 与 §11.6「有意保留的简化」同源，方向是更严。
- 行数棘轮：本轮在**已超限**的文件上小幅增长（`sync_service/*`、`event/reader.rs`、`unread.rs` 等），
  **未新增超限文件**（仍 `257 over 500 / baseline 253`）；未 `--update` —— 提升天花板会扩大免责面，
  与棘轮"只降不升"的意图相反。

---

### 11.11 第五轮（2026-10-14）：四条长期遗留项的 HEAD 重判与收口

主题：`RUSTSEC-2024-0449`、联邦 PDU、Room v12 剩余项（A3/A4/A5/C1）、E2EE v2.0 计划
（N-01 / M-01 / M-02 / M-03 / L-02）。方法是**在移动的 HEAD 上逐条重判**再动手 —— 事后证明有必要：
其中"联邦 PDU"一整行结论已经过时（§11.11.6）。

#### 11.11.1 RUSTSEC-2024-0449 —— 已消除（迁 `serde_norway`，不是加豁免）

| 项 | 判据 |
| --- | --- |
| 链路 | `serde_yaml 0.9.34+deprecated` → `unsafe-libyaml 0.2.11`（上游已归档）|
| 迁移目标 | `serde_norway 0.9.42` —— 同一份 0.9 代码的维护 fork，底层换 `unsafe-libyaml-norway 0.2.15` |
| 调用面 | **49 处 / 14 个 `.rs`**：`from_str` 32、`to_string` 13、`Value` 2、`Error` 1；**全仓无 `use serde_yaml` 别名** ⇒ 纯文本替换，无结构改动 |
| 依赖声明 | workspace `Cargo.toml` + 3 个 crate（common / services / web）|
| 锁文件 | 恰 **20 增 / 20 删** —— 只换掉那两个 package 块，无连带升级 |
| 未加豁免 | `.cargo/audit.toml` / `deny.toml` **均未**新增 ignore ⇒ 是消除而非遮蔽 |

#### 11.11.2 顺带修复：4 条**当日新发布**的 rustls 漏洞（非本轮引入）

迁移后 `cargo deny check advisories` 仍 FAILED，但命中项**不是** 0449：

| advisory | 受影响 | 修复区间 | 内容 |
| --- | --- | --- | --- |
| RUSTSEC-2026-0346 | rustls 0.23.45 | `>= 0.23.46` | `Debug` 实现泄漏 TLS 会话秘密 |
| RUSTSEC-2026-0347 | 同上 | 同上 | ECH 被拒后仍向外部服务器发送已缓冲的应用数据 |
| RUSTSEC-2026-0348 | 同上 | 同上 | Brotli 证书解压可在认证前分配至 1 GiB |
| RUSTSEC-2026-0349 | 同上 | 同上 | 未认证 ServerHello 可把后续连接降级到经典密钥交换 |

四条 `date = "2026-10-14"`（**当天发布**），而仓库上一次 advisory 复核是 2026-09-21
⇒ 属**既有暴露、新库才暴露**，非本轮引入。归属判据：`Cargo.lock` 的 diff **只有 yaml 两个 package 块**，
本轮未触及 rustls。

修复：补丁级升版 `cargo update -p rustls --precise 0.23.46`。rustls 是**传递依赖**
（`lettre`（SMTP）与 `redis` TLS），清单里没有可钉的位置；防降级由 CI 的 supply-chain gate
每次重跑覆盖，故不需要额外 pin。

**结果（两个工具独立确认）**：`cargo deny check advisories` → `advisories ok`（EXIT=0）；
`cargo audit --db <本地库快照> --no-fetch` → EXIT=0（载入 1314 条 advisory，零命中）。
⇒ **供应链门禁首次转绿**。

#### 11.11.3 E2EE v2.0 计划：五项逐条重判（4 项已修 + 1 项比报告更严重）

| 任务 | 结论 | 判据 |
| --- | --- | --- |
| **N-01**（空的 algorithm/key 默认值） | ✅ 已修 | `cross_signing/service.rs:198/202/208/212` 一律 400；`:215`/`:218` 另加 `algorithm.is_empty()` 与 `public_key.is_empty()` 两道闸 |
| **M-01**（配置解析静默回退） | ✅ 已修 | `key_rotation/service.rs:61, 68, 76, 84` 四处 `tracing::warn!` |
| **M-02**（时间戳静默回退） | ✅ 已修 | `megolm/storage.rs:40, 44` 两处 `tracing::warn!` |
| **M-03**（`log_rotation` 的 device_id） | ❌ **比报告更严重** ⇒ 本轮修 | 见 §11.11.4 |
| **L-02**（文档注释写 "Panics"） | ✅ 已修 | `crypto/key_at_rest.rs` 已无 `Panics` |

#### 11.11.4 M-03 实为「必然失败的 INSERT」—— 已修（不改 schema、不写占位值）

原报告只说"缺设备追迹"。实测：`key_rotation_log.device_id` 在基线里是 **`TEXT NOT NULL`**，而
`log_rotation` 绑定 `None::<String>` ⇒ **每次调用必然 `null value in column "device_id" violates
not-null constraint`**。未被发现的原因有两条：唯一调用者 `rotate_megolm_session` 在生产**没有接线**
（`KeyRotationService` 全仓只在测试里构造），且该模块此前**零 DB 覆盖**。

修法：把 `device_id` 做成**必填 `&str`**，从 `rotate_megolm_session` / `rotate_all_user_sessions`
一路透传 ⇒ **结构上不可能再绑 NULL**。刻意**不**写"unknown"之类的合成设备号：合成值在审计日志里
与真值不可区分，且与本仓 `pdu.rs`「**禁止**填 `[]`/`0` 伪造」是同一条纪律。读者侧
`get_device_rotation_history` 本就是 `(user_id, device_id)` 过滤，故这一改同时让读写语义对齐。

**新增守卫** `synapse-e2ee/src/key_rotation/service/db_tests.rs`：真库往返（`log_rotation` → 读回 1 条、
`new_key_id` 与 `rotated_at` 非空）+ **两个反向对照**（换设备、换用户都读不到）—— 一次钉住列的
`NOT NULL`、参数顺序与读者的过滤。实测 PASS（14.6s）。

**顺带修掉一处假告警**：`should_rotate` 的**正常**分支（没有任何触发器命中）挂着一句
`warn!("Timestamp conversion failed or expired_at is invalid; treating session as not expired")`
⇒ **每个健康会话都会刷**，而且是错误诊断（时间戳转换只在 `megolm/storage.rs` 的 row→model 边界发生一次，
失败在那里已自报）。已删除。

#### 11.11.5 本轮**新发现**、只登记不改（都是产品/模型决策，不是能"顺手修"的 bug）

| 编号 | 发现 | 判据 | 可选处置 |
| --- | --- | --- | --- |
| **M-04** | `KeyRotationConfig::max_session_age_days` **完全不生效** | 该字段被读取、持久化、经 API 暴露、并在 4 个测试里断言，但 `key_rotation/service.rs` 里**没有任何读者**；真正的最大寿命来自 `MEGOLM_SESSION_MAX_AGE_DAYS` 环境变量（默认 7 天）→ 写进 `expires_at`，再由 `should_rotate` 的 `expires_at` 分支生效 | 二选一：删掉这个旋钮，**或**让环境变量从它取值。**不能**两者并存 —— 同一策略两个真源，违反铁律 2 |
| **M-05** | `key_rotation_log` **在生产没有写入者**，而读者是活的 | 唯一写入者是未接线的 `KeyRotationService`；读者 `GET /_matrix/vendor/v1/keys/rotation/history/{device_id}`（`routes/key_rotation.rs:268`）经 `FederationKeyRotationService::get_device_rotation_history` ⇒ **该端点恒返回空列表**。且存在模型错配：联邦**签名密钥**是 per-server，而该读者按 `(user, device)` 过滤 | 先定模型 —— 这张表记的是 megolm 轮换（per user/device）还是联邦签名密钥轮换（per server），再接写者 |

#### 11.11.6 联邦 PDU：memory 里的结论**已过时**，在 HEAD 上重判为「已实施」

| 旧结论 | HEAD 实测 | 判据 |
| --- | --- | --- |
| `event_id` 非 v4+ reference hash | ✅ **已实施** | `synapse-federation/src/event_finalize.rs::finalize_local_pdu` 走完整链（内容哈希 → v3+ reference hash；v1/v2 保留服务端随机 ID），生产调用点至少 3 处：`room/lifecycle/create.rs:136`、`membership/federation.rs:864`、`services/graph_metadata.rs`。`crypto::generate_event_id` 已标 **DEPRECATED** 并指向 `event_id::compute_event_id` |
| 本地 `create_event` 不落 `depth`/`prev_events`/`auth_events` | ✅ **已实施** | `GraphMetadataWriter` 装饰器已在 `synapse-services/src/wiring/rooms.rs:122` 接线（该处是 `Arc<dyn EventWriter>` 的**唯一**装配点），`create_event_with_pdu` 供事务路径显式供给；`lifecycle/creation_graph.rs`、`state/auth_events.rs` 均已把旧行为写成缺陷 |

**仍成立的两条残留**（都不得用伪造绕过）：① 装饰器上线**之前**落库的历史行仍被判
`MissingGraphMetadata` ⇒ 拒签而非伪造（方向正确，但"老房间的入站状态无法验签"是实际影响，需要专门的
backfill 决策）；② 入站事件**不落远端 `signatures`/`hashes`** ⇒ 转发远端 PDU 只有本机签名。

#### 11.11.7 Room v12 剩余项：在 `main@3439a0ebd` 上重判

| 项 | 位置 | 结论 |
| --- | --- | --- |
| **A3** per-event state group | `synapse-storage/src/state_groups.rs`（954 行，API 完整）+ 基线四表 `state_groups` / `state_group_edges` / `event_to_state_groups` / `state_group_state` + `tests/integration/state_groups_idempotency_tests.rs` | **存储层已实现**，但**生产未接线** —— `synapse-services/src/room/state_record.rs:43` 自述 `event_to_state_groups` "today unused" |
| **A4** backfill | 依赖 A3 | **仍未做**（随 A3 的接线一起） |
| **A5** live 联邦互操作 | — | **未做**（需双机环境） |
| **C1** 陈旧 `.sqlx` 缩容 | `.sqlx/` 1318 条 | **本轮未复核** —— 需与 census 逐条对照，属独立批次；`sqlx_cache_tooling_guard_tests` 管的是**脚本行为**，不是条目新鲜度 |

#### 11.11.8 E5（本文件被"陈旧副本"覆盖）第 5 次：拿到了写入者的读窗口

本轮开工时工作区又是**"第三轮之前"的副本**（§11.6 回到"5 条读路径"、§11.7 回到"本轮未实施"、
§11.9 与 §11.10 **整段消失**），mtime **21:03:38**。

**新证据**：这份副本的标题里带着 `EXPERIMENT` 后缀 —— 那是本会话在**上一轮做对照实验时、只存在了
一两分钟的哨兵**。⇒ 写入者的缓冲区是在**那个窗口内**读的文件，之后才用陈旧内容回写。据此用该哨兵串
（含 JSON 转义形态）在 `~/.workbuddy`、`~/Library/Application Support`、`~/Library/Caches` 搜索，
**零命中** ⇒ 宿主不是这些目录里的明文文件（可能在进程内存，或压缩/私有格式里）。

处置仍按 §11.9 规程：`git checkout -- <file>`，**内容以 git 为准**；还原后核验 10 章节 / 45 条标题在位。

#### 11.11.9 本轮改动的验证（全部实跑）

- `cargo check --workspace --all-targets --features test-utils --all-features --locked` → **EXIT=0**
- `cargo clippy --workspace --all-targets --features test-utils --all-features -- -D warnings` → **EXIT=0**
- `nextest --test unit --features test-utils`（配置 / 限流 / 流水线 / 迁移 / 基线指纹子集）→ **75 passed**
- `nextest -p synapse-e2ee --lib --features test-utils -E 'test(key_rotation)'` → **13 passed**（含新 DB 守卫）
  - 首轮出现一次 `LEAK` 标记，指向 `test_should_rotate_false_when_not_expired`；**复跑两次均无 LEAK、13/13 通过**
    ⇒ 是 nextest 对 `build_service` 里共享 `CacheManager` 后台任务的**时序归属**，不是本轮引入
- `cargo deny check advisories` → `advisories ok`；`cargo audit` → EXIT=0（1314 条 advisory，零命中）
- 行数棘轮 → 仍 `257 over 500 (baseline 253)`，**未新增超限文件**（故不 `--update`）
