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
  2. **MSC4210**：默认推送规则仍带四条"提及类"规则，上游默认已移除。
  3. **MSC4452**：`preview_url` 仍被默认关闭的实验开关门控，上游已**稳定化**为 `m.preview_url` 能力。
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

| 编号 | 类型 | 内容摘要 |
| --- | --- | --- |
| 19354 | feature | MSC4140：新增**调度延迟事件的专用端点** |
| 19495 | removal | MSC4140：不再因"另一用户发来更新的状态事件"而取消本用户的延迟状态事件 |
| 20006 | bugfix | 超过 `media_upload_limits` 的媒体上传会在磁盘留下无 DB 记录的孤儿文件 |
| 20081 | bugfix | 增量 `/sync` 中，当离开者是该房最后一个本地用户时 `state_after` 缺 leave 事件 |
| 20143 | removal | 移除不稳定的 MSC2965 `.well-known` 端点 |
| 20170 | feature | **稳定化 MSC4222**：`/sync` 支持稳定 `use_state_after` 与 `state_after`（Matrix v1.16），移除 `msc4222_enabled` 开关 |
| 20202 | bugfix | 被服务端配置禁用的默认推送规则，经客户端 API 修改时应 404 而非成功 |
| 20220 | removal | 默认移除遗留"提及"推送规则（MSC4210）；`msc4210_enabled` 默认改为 `true` |
| 20243 | bugfix | `request_token_inhibit_3pid_errors` 默认改为 `true`，防 `/requestToken` 泄露邮箱是否已注册 |
| 20273 | bugfix | 不再为 `url: null` 的 appservice（双傀儡注册）排队事件/临时数据 |
| 20294 | misc | 给 Synapse 的 Rust 侧加 Clock 抽象，并在 rendezvous 处理器中使用 |
| 20295 | bugfix | MSC4354 Sticky Events 的历史可见性豁免**覆盖全部 client-server 端点** |
| 20298 | feature | 新增 `m.preview_url` 能力（稳定化 MSC4452） |
| 20302 | bugfix | SQLite→PostgreSQL 迁移时初始化 `un_partial_stated_room_stream_sequence` |
| 20312 | misc | 事件内部元数据结构加入 `received_ts` |
| 20315 | misc | 推送规则评估跳过 **sender 自身**的规则，提速 |
| 20321 | bugfix | 修复 Pydantic 校验错误信息向客户端泄露内部细节 |
| 20322 | bugfix | 同一持久化批次内被 redact 的 Sticky Event 不应 sticky |
| 20323 | bugfix | 持久化**之后**被 redact 的 Sticky Event 需移除 sticky 性 |
| 20324 | bugfix | 修复 Sticky Event 到期时间"上报值与服务端追踪值不一致" |
| 20328 | bugfix | 远端资料查询时过滤 MSC4426 User Status 字段（对不支持 MSC4259 的客户端会变陈旧） |
| 20331 | doc | 文档化 `/auth_metadata` 端点可由 worker 处理 |
| 20333 | feature | 宣告支持 Matrix **v1.16** |
| 20334 | feature | 宣告支持 Matrix **v1.17** |

---

## 3. 逐条对照（本仓实测）

### 3.1 结论汇总表

| 上游条目 | 本仓现状（实测位置） | 判定 | 行动 |
| --- | --- | --- | --- |
| 20333 / 20334 | `/versions` 宣告 `v1.1`…`v1.14`（`synapse-services/src/capability_governance.rs:68-87`） | **差距** | 批次 A |
| 20170 | 已支持稳定 + 不稳定双拼写（`synapse-web/src/routes/handlers/sync.rs:237-241`） | 基本对齐 | 仅补测试 |
| 20081 | 待专项验证 | 待核实 | 批次 A（测试） |
| 20220 | 默认规则仍含 4 条提及规则（`synapse-common/src/push_rules.rs:21,70,83,94`） | **差距** | 批次 B |
| 20298 | 已实现但被 `msc4452_enabled` 门控，默认 `false`（`synapse-common/src/config/experimental.rs:17,54`；`synapse-web/src/routes/media/preview.rs:36`） | **差距** | 批次 B |
| 20295 / 20322 / 20323 / 20324 | 有 MSC4354 实现（`synapse-web/src/routes/room.rs:82-95,169`），4 条修复未见对应收口 | **疑似差距** | 批次 C |
| 19354 / 19495 | 发送端 `org.matrix.msc4140.delay`（`synapse-web/src/routes/handlers/room/mod.rs:141`）+ 管理端点（`synapse-web/src/routes/delayed_events.rs:97-98`）+ storage + 联邦 EDU | 待核实 | 批次 C |
| 20273 | appservice 注册模型 `url: String` **必填**（`synapse-services/src/application_service/models.rs:41-42`） | **兼容性差距** | 批次 B |
| 20243 | 未找到该配置项或其等价防枚举逻辑 | 待核实 | 批次 B |
| 20006 | 全局 body 上限由路由层前置拦截（`src/server/router.rs:37-39`）⇒ 超限在落盘前 400/413 | 不适用 | 无 |
| 20321 | `ApiError::message()` 对 `Internal` 返回泛化文案，细节只进日志（`synapse-common/src/error.rs:888-900`） | 已对齐 | 无 |
| 20202 | 本仓无"配置禁用 server-default 推送规则"的开关 | 不适用 | 无 |
| 20143 | `.well-known/matrix/client` 仅回 `m.homeserver` / `m.tile_server`（`synapse-web/src/routes/handlers/versions.rs:142-154`） | 已对齐 | 无 |
| 20302 | 本仓 PG-only，无 SQLite 迁移路径 | 不适用 | 无 |
| 20312 | 本仓无 `EventInternalMetadata` 结构 | 架构差异 | 无 |
| 20315 | 未定位到"按 sender 跳过规则"的评估路径 | 待核实 | 低优先 |
| 20328 | 本仓无 MSC4426 User Status 功能 | 不适用 | 无 |
| 20294 | Rust 侧无统一 Clock 抽象 | 可选改进 | 低优先 |
| 20331 | 有 worker 门控；`/auth_metadata` 是否可由 worker 服务未标注 | 低优先 | 低优先 |

### 3.2 重点条目明细

#### 3.2.1 规格宣告落后（20333 / 20334）—— 最高性价比

本仓 `CLIENT_API_VERSION_SUPPORT` 的注释写着"只宣告 v1 家族"，但列表**止于 `v1.14`**，且 `v1.14` 那条还带"仅 1 个niche 缺口"的 2026-06-28 遗留说明。上游已并列宣告 **v1.16 与 v1.17**。

注意这**不是**"改个常量"那么简单：宣告一个版本等于承诺该版本引入的端点/字段可用。因此该项与 §2 中的 MSC4222 稳定化（v1.16）耦合，应同批处理，并配一条"宣告版本 ⇔ 端点存在"的守卫（见批次 A）。

#### 3.2.2 MSC4210 提及规则（20220）

`synapse-common/src/push_rules.rs` 的 `default_push_rules_for_user()` 当前产出：

- `.m.rule.contains_user_name`（第 21 行）
- `.m.rule.is_user_mention`（第 70 行）
- `.m.rule.contains_display_name`（第 83 行）
- `.m.rule.is_room_mention`（第 94 行）

上游 MSC4210 把这四条从**默认**集合移除（改为需要显式配置才下发）。这是一条会直接改变客户端通知行为的语义变更，**不宜静默改默认值**——建议引入 `msc4210_enabled`（默认 `true`，与上游一致），保留回退开关一个发布周期。

#### 3.2.3 MSC4452 `preview_url` 稳定化（20298）

本地已有端点与 403 门控，但能力名是实验性的 `io.element.msc4452.preview_url`，且默认关闭（`msc4452_enabled: false`）。上游将其稳定为 `m.preview_url` 能力。需要确认的是：**稳定化后是"能力名改变"还是"默认开关改变"**（或两者）。核实方法见 §6。

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

4. **B1（MSC4210）**：新增配置 `experimental_features.msc4210_enabled`，默认 `true`；为 `true` 时 `default_push_rules_for_user()` 不下发四条提及规则。默认值变更需在 CHANGELOG 与 `docs/` 明写。
5. **B2（MSC4452）**：按 §6 的核实结果决定——稳定能力名 `m.preview_url`，并明确默认开关语义。
6. **B3（appservice `url: null`）**：`url` 改为 `Option<String>`；`None` 时**跳过**事件/临时数据排队（对齐 20273），并在启动日志标注"该 appservice 不接收推送"。
7. **B4（20243）**：核实本地 `/requestToken` 是否可枚举账号；若是，加 `request_token_inhibit_3pid_errors`（默认 `true`）。

- 验收：每条配一个"行为可观察"的测试（不是断言常量）；B1/B3 需要变异自证。

### 批次 C —— 深度收口（P2）

8. **C1（MSC4354 四条）**：逐条写回归（redact 同批次 / redact 后 / 到期口径 / 历史可见性豁免覆盖全部 C-S 端点），失败的按上游修法修。
9. **C2（MSC4140）**：对照上游 19354 PR 确认"专用调度端点"与本地"发送体 delay 提示"是否等价；核对 19495 的取消语义（本地只有显式取消 ⇒ 需证明不存在隐式取消路径）。
10. **C3（20315）**：定位推送评估路径，确认是否已跳过 sender 规则；未跳过则补。
11. **C4（可选）**：Rust 侧 Clock 抽象（20294）——提升 rendezvous 的可测性，非阻塞。

### 批次 D —— 门禁与文档（与 A/B/C 同步）

12. 每批完成后更新 `docs/audit/INDEX.md`、`CHANGELOG.md`，并复跑全套 preflight（见 §5）。

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

| # | 问题 | 核实命令 / 方法 |
| --- | --- | --- |
| V1 | 上游 v1.15/v1.16/v1.17 相对 v1.14 新增了什么 | 读 `matrix-spec` 对应版本的 changelog；或对比上游 `synapse/rest/client/versions.py` |
| V2 | MSC4452 稳定化是否改默认开关 | 读上游 PR #20298 与其 `experimental` 配置 diff |
| V3 | MSC4140 是否需要"专用端点" | 读上游 PR #19354 的 REST 路由 diff，比对本地 `delayed_events.rs` |
| V4 | 本地 `/requestToken` 是否泄露账号存在性 | 对已注册/未注册邮箱各发一次 `/account/3pid/email/requestToken`，比较响应 |
| V5 | 本地推送评估是否跳过 sender | `rg -n "sender" synapse-services/src/push* synapse-services/src/sync_service` |
| V6 | 20315 的提速是否适用本仓 | 同 V5 |
| V7 | `/auth_metadata` 能否由 worker 服务 | `synapse-web/src/routes/worker.rs` 的门控清单是否含该路径 |

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

| 测试 | 作用 |
| --- | --- |
| `declared_versions_have_evidence` | 两表 1:1（同序、同长），证据非空/非占位，且必须引用自身 changelog |
| `the_evidence_checker_rejects_a_copy_pasted_row` | 红证明：复制相邻行 ⇒ 判红 |
| `the_evidence_checker_rejects_a_missing_row` | 红证明：漏写一行 ⇒ 判红 |
| `the_evidence_checker_rejects_a_placeholder` | 红证明：证据里留 `TODO/FIXME/XXX` ⇒ 判红 |

"引用**自身** changelog"这一条是关键：它让"复制上一行改个版本号"这种最常见的敷衍失效。

### A3 —— 20081 增量 `/sync` 的 `state_after` 回归

`synapse-storage/src/event/db_tests.rs` 新增
`state_events_since_batch_keeps_the_last_local_users_leave`：构造"房内最后一个本地用户 leave"，在 leave **之前**截取增量游标，断言 `get_state_events_since_batch` 的结果里含该用户的 `m.room.member`(leave)，且**不含**游标之前的 join（证明下界真的生效）。

**结论：本仓结构上不受 20081 影响。** 根因：`get_state_events_since_batch` 是 `stream_ordering > since` 的**无上界**查询，且**不按当前成员过滤**（`synapse-storage/src/event/state.rs`），所以离开事件必然进入 `state_after`。上游的缺陷来自"取时间线**起点**的状态"这一有界计算，本仓没有该计算。

### 验证

| 命令 | 结果 |
| --- | --- |
| `cargo nextest run -p synapse-services -E 'test(evidence) or test(versions) or test(r0_versions)'` | **7 passed**（含 3 个红证明） |
| `cargo nextest run -p synapse-storage -E 'test(state_events_since_batch_keeps_the_last_local_users_leave)'` | **1 passed**（240s，DB 模板克隆较慢） |
| `cargo check -p synapse-storage --all-targets --features test-utils` | exit 0，0 警告 |

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
