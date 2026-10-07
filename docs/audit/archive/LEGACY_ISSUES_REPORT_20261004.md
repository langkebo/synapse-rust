# synapse-rust 遗留问题排查报告

> 基线版本：v6.2.0
> 分支：`feature/2026-10-01-metrics-docs-updates`（HEAD `973ac3a0c`，工作区干净）
> 排查日期：2026-10-04（第二轮全面系统复核）
> 结论：**无 P0 阻断项；第二轮全部遗留问题已处置完毕（2026-10-04 末次收尾后 P1×0 / P2×0 / P3×0）**
> 说明：本轮在首轮清单基础上做了二次复核与扩面取证。首轮正文 36 项（头部曾误计 31）中 27 项已修复/归档（含本轮补修的 F1）、9 项仍存续，另新发现 59 项。
> 修复进展：`C7`（SAML 响应签名验证 fail-open）已于 2026-10-04 修复为 fail-closed；`F1`、`F9–F12`（文档路径漂移/索引失真/SDK r0 前缀/口径矛盾）已于同日完成纠偏，**P1 级问题全部清零**，详见 §四 F 表与 §七；同日完成 **P2 代码批次**（`A3/A6/A7`、`B8/B9/B12/B13/B14`、`E7/E8/E9`，共 11 项）处置，**P2 由 28 降至 17**，详见 §四 与 §八；同日续完成 **P2 安全配置批次**（`C5/C8/C9`，共 3 项），**P2 进一步降至 14**，详见 §四 C 表与 §七；同日末完成 **P2 lint 批次**（`E1`，共 1 项，有限消除 + 精确归档：264 处口径 + 重构批次计划），**P2 进一步降至 13**，详见 §四 E 表与 §八；同日末完成 **P3 收尾批次**（`D12` 修复、`D14` 订正，共 2 项），**P3 全部清零（34/34）**，详见 §四 D 表与 §七；同日末完成 **P2 兼容性对齐批次**（`D5–D10/D13`、`F13–F18`，共 13 项：D5/D6/D7/F14/F15 证伪、D8/D10/D13/F13/F16/F17/F18 修复或澄清、D9 订正），**P2 全部清零（13→0）**，详见 §四 D/F 表与 §七。

---

## 一、排查方法与范围

| 手段 | 执行情况 | 结果 |
|------|----------|------|
| 代码审查 | 9 包 workspace 全量 + 6 领域并行子代理二次取证 | 完成，文件:行号级证据 |
| 静态门禁 `cargo clippy --workspace --all-targets --all-features -D warnings` | 已运行（最近一次 push 时） | **0 警告，exit 0** |
| 依赖安全 `cargo audit` | 已运行 | **干净**，624 依赖无已知漏洞（网络抖动会使 pre-push 失败） |
| `unsafe` 棘轮（geiger） | 读取基线 | 生产 **0**，测试 9 |
| 债务标记扫描 | TODO/FIXME、`unimplemented!/todo!`、`unwrap/expect` | 生产实质缺口 ≈0，见 §八 |
| 配置/部署审查 | homeserver.yaml、rate_limit.yaml、deploy.sh、.env.example | 完成 |
| 文档死链/口径审查 | docs/（>200 份 Markdown） | 完成，发现多处索引与实现漂移 |
| 既有结论复核 | 对首轮 31 项逐条在 HEAD `973ac3a0c` 复验 | 完成，见 §五 |
| 运行时验证 | 上一轮 `deploy.sh --all` 部署 | 23 步成功、容器 healthy、健康检查全 200、DB 一致性通过 |

**方法学边界**：本轮为「代码审查 + 静态工具 + 配置审查 + 既有部署健康验证」，**未**重跑完整功能/集成/性能基准测试；标注为「潜在/待确认」的项为静态推断，需运行期确认。子代理相互冲突的结论（如 `docs/sdk/errors.md`）已由主控逐条取证裁决。

---

## 二、总体结论（正面基线）

- **编译/静态质量已达发布级**：clippy 全绿（含 `unwrap_used/expect_used = deny` 门禁）、audit 无漏洞、生产 `unsafe = 0`。
- **核心安全链路扎实**：JWT（HS256+exp/iat/sub+iss/aud）、refresh token CAS 轮换 + 重用全撤销、SQL 全参数绑定、SSRF 防护、安全响应头齐全、Server ACL fail-closed；SQLi/路径穿越/命令注入/SSRF/TLS 校验/AEAD/IDOR 经复核均无问题。
- **无 P0**：不存在阻断发布的缺陷；问题集中在**运行时内存放大、认证降级与 fail-open 残留、文档可信度、部分功能未接线**四类。
- **本轮最高优先级**：`C7`（SAML 签名验证可被静默跳过，安全 fail-open）与 `F1`、`F9–F12`（索引/文档与实现漂移）均已于 2026-10-04 修复，**P1 级问题全部清零**；剩余问题集中在 P2/P3。

---

## 三、优先级总览

| 级别 | 数量 | 主题 |
|------|------|------|
| **P1 高** | 0 | 已全部清零（`C7`、`F1`、`F9–F12` 均于 2026-10-04 修复） |
| **P2 中** | 0 | r0 兼容与 i18n/错误码/版本口径（D5–D10/D13）、文档死链与口径冲突（F13–F18）——**已全部清零（2026-10-04）**<br>**已处置（2026-10-04，P2 代码批次，11 项）**：A3/A6/A7、B8/B9/B12/B13/B14、E7/E8/E9<br>**已处置（2026-10-04，P2 安全配置批次，3 项）**：C5/C8/C9<br>**已处置（2026-10-04，P2 lint 批次，1 项）**：E1（有限消除 + 精确归档：264 处口径 + 重构批次计划见 §八）<br>**已处置（2026-10-04，P2 兼容性对齐批次，13 项）**：D5/D6/D7（证伪：证据过时）、D8/D10/D13（修复）、D9（订正）、F13（修复：issues 死链）、F14/F15（证伪：证据过时）、F16/F17/F18（修复/澄清） |
| **P3 低** | 34 | 次要功能缺口/性能/配置/注释/文档噪音（A5/A8–A14、B4/B5/B7/B10/B11/B15–B17、C10–C14、D11/D12/D14、E10–E15、F8/F19–F21）<br>**已处置（2026-10-04，P3 安全/配置批次，5 项）**：C10/C11/C12/C13（修复）、C14（核实：VAL-01/DEP-01 已满足、Abuse-01 设计取舍）<br>**已处置（2026-10-04，P3 严格化批次，5 项）**：A9/A10/A12/A13/A14<br>**已处置（2026-10-04，P3 内存/文档批次，9 项）**：B4/B5/B7/B10/B11/B15（修复）、B16/B17（核实：有界/test-only，非缺陷）、D11（证伪：证据过时）<br>**已处置（2026-10-04，P3 代码质量批次，6 项）**：E10（修复：样板就地提取 helper）、E11（归档：设计取舍）、E12（归档：按端点设计）、E13（订正：无生产逃逸）、E14（归档：test-only+守卫）、E15（归档：受守卫的动态 SQL）<br>**已处置（2026-10-04，P3 文档批次，4 项）**：F8（归档：索引已覆盖+只读声明，补 `plans/` 索引项）、F19（归档：归档冻结、只读不维护）、F20（澄清：`CHANGELOG` 头部补历史路径说明）、F21（归档：现行覆盖分散于既有索引文档）<br>**已处置（2026-10-04，P3 收尾批次，2 项）**：D12（修复：迁移回滚文档改为真实机制）、D14（订正：拆分上游跟踪目标 v1.19 与本仓声明基线 v1.14，确立单点权威） |

> 与首轮对比：P1 由 5→6，P2 由 14→28，P3 由 12→34。增量主要来自首轮未覆盖的**运行时无界增长点**、**SAML/认证降级面**、**r0 兼容残留**与**文档索引失真**。（P2 复核对齐后，已随 2026-10-04 P2 代码批次处置 11 项降至 17，续随 P2 安全配置批次处置 3 项（C5/C8/C9）降至 14，随 P2 lint 批次处置 1 项（E1）降至 13，末随 P2 兼容性对齐批次处置 13 项（`D5–D10/D13`、`F13–F18`）降至 **0**。）

---

## 四、分领域详细清单

> 状态图例：🆕 本轮新发现｜⚠️ 首轮项仍存续｜✅ 已在本轮前修复（仅列于 §五）
> 「严重度」综合影响面与可利用性；「优先级」为处置排序依据。

### A. 功能完整性与正确性（代码缺陷）

**A1｜`m.delayed_event` EDU 只校验不持久化** — 中｜✅ 已按口径校正
- 证据：`synapse-web/src/federation/edu.rs:761-822`
- 处置：`d3976ca86` 校正指标/日志口径（如实声明「接收但未落库」），确认为设计边界。

**A2｜ClamAV socket 路径硬编码** — 中｜✅ 已修复
- 证据：`synapse-services/src/content_scanner/service.rs`（原 L55 硬编码）
- 处置：`bdbe57c64` 改为从 `ContentScannerConfig` 透传 socket 路径。

**A3｜voice 三端点恒返回 501** — 中｜✅ 已处置（2026-10-04，口径对齐）
- 证据：`synapse-web/src/routes/voice.rs`（convert/optimize/transcribe 仍注册并返回 `M_UNRECOGNIZED` 501）
- 处置：**最小侵入**——保持 501 运行时行为不变，在 `get_voice_config` 增补 `server_side_processing:{convert:false,optimize:false,transcription:false}` 能力声明，并加文档注释说明三端点按 MSC3245 由客户端处理、服务端故意不实现；SDK 可据能力声明 gate 调用，消除「能力已注册→调用硬失败」的误导。`tests/integration/voice_routes_tests.rs` 补 3 条断言。

**A4｜事务幂等非原子** — 中｜✅ 已修复
- 证据：原 `synapse-services/src/room/messaging/messages.rs:328-348`
- 处置：`973ac3a0c` 将事务去重 marker 与事件写入同一事务。

**A5｜admin 媒体端点 `users` / `user` 单复数不一致** — 低-中｜✅ 已澄清（非缺陷，2026-10-04）
- 证据：`docs/synapse-rust/ROUTE_CONTRACT.md` admin media 段
- 处置：核对上游 `synapse/rest/admin/media.py` 与 `synapse/rest/admin/user.py`——`/users/{user_id}/media`（GET/DELETE，列/删用户媒体）确为**复数** `users`，而 `/user/{user_id}/media/quarantine`（按用户隔离）为**单数** `user`。两处单复数差异**忠实镜像上游**，非本仓缺陷；已随 A8 一并在契约与覆盖率报告中按实测口径写明。**结论：无需改代码。**

**A6｜MSC4140 state 推迟路径未实现 + `state_key` 硬编码 `None`** — 中｜✅ 已处置（2026-10-04，口径校正 + 子特性补齐）
- 证据：`synapse-web/src/routes/handlers/room/events.rs`（delay 调度仅在消息发送端点 `send_message`，其 path 无 `state_key` 入参，故 `state_key: None` 正确、**不会污染房间状态内容**）；state 端点 handler（`handlers/room/state.rs`）无 delay 逻辑
- 处置：经代码取证，原报告「污染房间状态内容」不成立（被审计路由为消息端点）。真实缺口为 **MSC4140 state-key 延迟事件子特性未实现**。
- 补齐（2026-10-04）：已实现 state-key 延迟事件路径——抽出共享 helper `schedule_delayed_event_if_requested`（`handlers/room/mod.rs`；消息路径改调它，行为不变），并为 `/state` 写端点（`put_state_event`、`put_state_event_empty_key`、`put_state_event_no_key`、`send_state_event`）接入 delay 分支，透传真实 `state_key`；后台 dispatcher（`src/server/mod.rs:837-871`）既有的 `state_key` 分支据此走 `create_event` 状态事件路径。`docs/synapse-rust/MSC_SEMANTICS.md` MSC4140 已由 🟡 PARTIAL 更新为 ✅ 已实现；新增集成测试 `tests/integration/api_delayed_state_event_tests.rs`（4 条，真实 DB 运行通过）。

**A7｜JSON 提取器未归一化为 Matrix 错误体** — 中｜✅ 已处置（2026-10-04，错误体归一化）
- 证据：`synapse-web/src/routes/extractors/json.rs`（`MatrixJson`）、`synapse-common/src/error/code.rs`（`M_NOT_JSON`/`M_BAD_JSON` 注释曾颠倒）
- 处置：`MatrixJson` 提取器错误映射归一化——`JsonDataError`（合法 JSON、结构/类型不符）→ `M_BAD_JSON`；`JsonSyntaxError`/`MissingJsonContentType`/其它（非合法 JSON、缺 Content-Type、body 不可读）→ `M_NOT_JSON`；订正 `MatrixErrorCode` 中 `BadJson`/`NotJson` 的 doc 注释，4 个单测补错误码断言。**遗留**：路由层仍有约 261 处裸 `Json`（81 文件）未迁移至 `MatrixJson`，转后续批次。

**A8｜admin media 路由面与上游偏差（protect/unprotect、room 单复数、server_name、quarantine_changes）** — 低-中｜✅ 已完成（2026-10-04，全量对齐上游）
- 证据：`synapse-web/src/routes/admin/media.rs`、`docs/synapse-rust/ROUTE_CONTRACT.md` admin media 段
- 处置：以 element-hq/synapse `synapse/rest/admin/media.py`（develop）为基准**全量对齐**：
  1. `protect`/`unprotect` 均**去掉** `{server_name}` → `POST /_synapse/admin/v1/media/protect/{media_id}`、`.../unprotect/{media_id}`（上游本就没有该段）；
  2. `rooms` → `room`（单数）：`GET /_synapse/admin/v1/room/{room_id}/media`、`POST .../room/{room_id}/media/quarantine`、`POST .../room/{room_id}/media/unquarantine`、`DELETE .../room/{room_id}/media/{media_id}`；
  3. `GET`/`DELETE /_synapse/admin/v1/media/{server_name}/{media_id}` 补回 `{server_name}`；
  4. 全局隔离变更列表由旧 `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes` 改为 `GET /_synapse/admin/v1/media/quarantine_changes`（query `from` 默认 0、`limit` 固定 100，响应 `{next_batch, changes:[{origin,media_id,quarantined}]}`）。
- 同步：`synapse-services/src/admin_media_service.rs`（新增 `get_quarantine_changes`）、`synapse-web/src/utils/admin_auth.rs`（补 `/_synapse/admin/v1/room/` 前缀覆盖）、6 份 ledger fixtures、`derived_route_table_*.inc.rs`、`route-table.json`、`ROUTE_CONTRACT.md`、2 份 route_ledger snapshot；契约 gate 全阶段通过（仅剩未提交 diff）。

**A9｜`/messages` 的 `filter` 参数被完全忽略** — 低-中｜✅ 已处置（2026-10-04，严格化）
- 证据：`synapse-web/src/routes/handlers/room/events.rs`（`get_messages`）
- 处置：本服务器在 `/messages` 与房间时间线端点**不支持** `filter`。不再静默忽略——`filter` 存在且非空（非 `null`/空串）时显式返回 `M_INVALID_PARAM`（400）；缺省或空值视为未提供，行为不变。

**A10｜`/messages` 的 `dir`/`from` 非法值静默回退** — 低｜✅ 已处置（2026-10-04，严格化）
- 证据：`synapse-web/src/routes/handlers/room/mod.rs`（`parse_pagination_direction`/`parse_room_messages_from_token`）、`handlers/room/events.rs`、`synapse-services/src/room/messaging/messages.rs`
- 处置：`dir` 仅接受 `f`/`b`，缺省为 `b`，其余值返回 `M_INVALID_PARAM`；`from` 缺省/空串解析为 `None`，无法解析的游标返回 `M_INVALID_PARAM`。服务层 `get_room_messages` 二次兜底，非 `f`/`b` 同样显式报错（不再静默归一化）。`/messages` 与房间时间线端点均接入严格解析。

**A11｜voice 路由 `Path<RoomId>` 与 `{media_id}` 语义不符 + 配置硬编码** — 低｜✅ 已处置（2026-10-04）
- 证据：`synapse-web/src/routes/voice.rs`
- 处置：(1) 语义修正——四个 `{media_id}` 路由 handler（content/convert/optimize/transcription）的路径参数由 `Path<RoomId>` 改为 `Path<MediaId>`，与路由模板一致；(2) 去硬编码——`get_voice_config` 的 `max_size_bytes` 与 `upload_voice_message` 的尺寸上限改由权威配置 `config.server.max_upload_size`（G-1 单一来源）派生（原硬编码 50 MiB=52,428,800，与配置默认 50,000,000 不一致）。`tests/integration/voice_routes_tests.rs` 断言改读同一配置，避免再次硬编码。

**A12｜`get_room_events_paginated_with_filter` 的 `to`/`filter` 仅告警忽略** — 低｜✅ 已处置（2026-10-04，严格化）
- 证据：`synapse-storage/src/event/pagination.rs`
- 处置：`to`/`filter` 不再仅 `warn` 后继续——显式返回 `sqlx::Error::Protocol`；非法 `from` 游标同样报错。该方法生产路径无调用者（仅 storage 内部测试以 `None` 调用，行为不变）。

**A13｜`task_queue::submit_delayed` 队列关闭时静默丢弃任务** — 低-中｜✅ 已处置（2026-10-04）
- 证据：`synapse-common/src/task_queue.rs`（`submit_delayed`，`#[cfg(test)]` 测试专用实现）
- 处置：延迟投递在 `submit_delayed` 返回后才发生，异步 spawn 无法将 `send` 失败同步回传；改为对失败记录 `tracing::error!`（含错误与原因），消除无痕丢失。生产路径使用 `RedisTaskQueue`，不受影响。

**A14｜captcha/SMS 未配置时返回 501** — 低｜✅ 已确认（非缺陷，2026-10-04）
- 证据：`synapse-services/src/captcha_service.rs`（SMTP 未启用 / 任务队列缺失 / SMS provider 未配置三处 `ApiError::not_implemented`）
- 处置：核对错误码体系——`ApiError::not_implemented` 本就映射 501 + `M_UNRECOGNIZED`（`MatrixErrorCode::Unimplemented`），与原报告建议一致，**无需改代码**；`tests/integration/captcha_tests_migrated.rs` 已断言 `is_not_implemented()`。

> 正面：Content Scanner、缩略图 `animated`、event_id reference hash 均已接线/解决（见 §六）。

### B. 性能与稳定性

**B1｜指标直方图内存无界增长** — 中-高｜✅ 已修复
- 证据：`synapse-common/src/metrics.rs`（原 L157-190）
- 处置：`654784d1e` 改为内存有界化。

**B2｜缓存前缀操作 O(n) 全量扫描** — 中｜✅ 已修复
- 证据：`synapse-cache/src/manager.rs`、`local.rs`
- 处置：`e0ea8e876` 按命名空间收窄扫描。

**B3｜大表查询无 LIMIT** — 中｜✅ 已修复
- 证据：`synapse-storage/src/event/state.rs`（原 L296,340）
- 处置：`8c406b4ba` 加防御性行上限。

**B4｜全表 COUNT(\*)** — 低-中｜✅ 已处置（2026-10-04）
- 证据：`synapse-storage/src/event/basic.rs`（`get_total_message_count`）
- 处置：经全仓检索确认该全表 `COUNT(*)` over `events` **仅 db_test 可达、无任何生产调用方**；加 `#[cfg(test)]` 门控，使其不再进入服务端二进制，测试覆盖保留。

**B5｜按 origin 的 Semaphore HashMap 无驱逐** — 低-中｜✅ 已处置（2026-10-04）
- 证据：`synapse-web/src/routes/federation/transaction.rs`（`acquire_origin_edu_permit`）
- 处置：为 `federation_inbound_edu_origin_semaphores` 加硬上限 `MAX_ORIGIN_EDU_SEMAPHORES = 4096`；超限时**优先驱逐空闲条目**（`available_permits() == per_origin_limit`，即无在途许可——保证驱逐不会让并发突发突破 per-origin 上限），若仍满则丢弃任一条目，in-flight 持有者保有自己的 `Arc`，仅回收 map 绑定、不释放 Semaphore。

**B6｜构建慢（工程效率）** — 中｜✅ 已缓解
- 说明：release profile `lto=true/codegen-units=1`；已由 Dockerfile 构建策略缓解。

**B7｜media `get_stats` 在 async 内同步递归 IO** — 低（当前仅测试可达）｜✅ 已处置（2026-10-04）
- 证据：`synapse-storage/src/media/filesystem.rs`
- 处置：把递归遍历 `process_directory` 提升为模块级自由函数，`get_stats` 经 `tokio::task::spawn_blocking` 在阻塞池执行 `std::fs` I/O，避免大树遍历阻塞 async runtime；失败经 `ApiError::internal_with_cause` 上抛。

**B8｜`room_operations_total` 的 `room_version` 标签源自用户可控未校验输入** — 中｜✅ 已处置（2026-10-04）
- 证据：`synapse-services/src/room/lifecycle/create.rs:12,39-51`
- 处置：引入 `is_supported_room_version` 枚举白名单，标签值仅接受受支持的 room version，其余（含畸形/任意值）一律折叠为 `unknown`，`None` 保持 `DEFAULT_ROOM_VERSION`，消除指标基数放大面；`room/lifecycle/tests.rs` 断言同步更新。

**B9｜FederationClient 两个 HashMap 缓存无上限** — 中｜✅ 已处置（2026-10-04）
- 证据：`synapse-federation/src/client.rs`
- 处置：为两个 origin 键缓存加上容量上限，超限时按「最旧/任意项」驱逐（不引入 LRU 依赖），保证长跑实例不再单调增长。

**B10｜KeyRotationManager.memory_cache 只写不读** — 低｜✅ 已处置（2026-10-04）
- 证据（订正）：`synapse-federation/src/key_rotation.rs`（原 `memory_cache` / `CachedKeyEntry` / `cache_historical_key`）
- 处置：该缓存唯一写入方法 `cache_historical_key` **无任何调用者/读取路径**，纯内存泄漏点；整体移除字段、类型别名与写入方法。

**B11｜`federation_presence_backoff_until` 无清理** — 低｜✅ 已处置（2026-10-04）
- 证据：`synapse-web/src/federation/edu.rs`（`set_presence_backoff`）
- 处置：每次写入前 `retain(|_, &mut v| v > now)` 丢弃已过期条目，使 map 规模以「当前处于退避期的 origin 集合」为界，不再随进程生命周期内见过的每个 origin 单调增长。

**B12｜EventBroadcaster.pending_queue 无上限** — 中｜✅ 已处置（2026-10-04）
- 证据：`synapse-federation/src/event_broadcaster.rs`
- 处置：为 `pending_queue` 加容量上限 + 溢出丢弃策略，避免订阅端阻塞时队列无界增长。

**B13｜RoomService.active_tasks 无清理，`cleanup_completed_tasks` 无调用点** — 中｜✅ 已处置（2026-10-04）
- 证据：`synapse-services/src/room/messaging/burn_after_read.rs:80-86`
- 处置：在每次插入新任务句柄前 `retain(|_, h| !h.is_finished())` 清理已完成句柄，`active_tasks` 由「单调增长」变为「以未完成烧毁任务数为界」；补 `process_read_receipt_prunes_finished_tasks` 单测。

**B14｜media 服务每次查找全量 `read_dir` 扫描** — 中｜✅ 已处置（2026-10-04）
- 证据：`synapse-services/src/media_service.rs:16-19,440-447`
- 处置：引入**有界进程内缓存** `file_name_cache`（上限 `MAX_FILE_NAME_CACHE_ENTRIES = 4096`，超限任意驱逐，miss 时回退目录扫描），消除每次查找的 O(n) 全量扫描；驱逐仅影响命中率、不影响正确性。

**B15｜`validate_push_gateway_url` 在 async 上下文执行阻塞 DNS** — 低｜✅ 已处置（2026-10-04）
- 证据：`synapse-services/src/push/gateway.rs`（`validate_push_gateway_url`）
- 处置：函数改 `async`，`to_socket_addrs` 移入 `tokio::task::spawn_blocking`，避免阻塞 async runtime；解析失败保持**非致命**（best-effort，TLS 握手兜底）；3 处调用方与 5 个单测同步 async 化。

**B16｜外部服务 health_status 无上限** — 低｜✅ 已核实（非缺陷，2026-10-04）
- 证据：`synapse-services/src/external_service_integration.rs:157,287,562`
- 核实：`health_status` 以 `as_id` 为键，仅在**管理员成功注册 appservice** 时插入、删除服务时同步 `remove`，键集以持久化的已注册服务数为界，非由不可信远端输入驱动；风险不成立，不施加人为上限（强加会破坏合法的多 appservice 部署）。

**B17｜InMemoryDeadLetterQueue 无上限** — 低｜✅ 已核实（非生产，2026-10-04）
- 证据：`synapse-federation/src/dead_letter_queue.rs:104,126,185`
- 核实：`InMemoryDeadLetterQueue` 文件头明示为 **test double**（"for tests"），生产路径使用持久化的 `PgDeadLetterQueue`；内存实现仅在 `#[cfg(test)]` 单测实例化，不进入服务端二进制。

> 正面：规则 §4.3 必需索引全部存在；连接池自洽（max_size=50）；无 `SELECT *`/N+1/连接泄漏。

### C. 安全性

**C1｜key_rotation 绕过 RBAC/MFA** — 中｜✅ 已修复（MFA 残留见 C9）
- 证据：原 `synapse-web/src/routes/key_rotation.rs:273-294`
- 处置：已改走完整授权路径。

**C2｜登录锁定 Redis 故障时 fail-open** — 中｜✅ 已修复
- 证据：原 `synapse-common/src/config/security.rs:123-126`

**C3｜限流 fail-open** — 中｜✅ 已修复
- 证据：原 `docker/config/rate_limit.yaml:75`

**C4｜TURN 弱默认密钥** — 中｜✅ 已修复（2026-10-04）
- 原证据：`docker/deploy/.env.example` 的 `TURN_SHARED_SECRET=dev-turn-secret`（弱默认密钥，随示例复制进 `.env` 后静默生效）
- 修复：`docker/deploy/.env.example` 与根 `.env.example` 统一为占位符 `__CHANGE_ME_match_coturn_static_auth_secret__`（要求替换为强随机值并同步 coturn `static-auth-secret`）；`deploy.sh` 去除 `:-dev-turn-secret` 静默兜底，改为「未设置/仍为占位符 → `log_error` 显式报错」，并停止在部署摘要中回显密钥值；`docker/deploy/README.md` 参数表与配置对应关系同步。应用启动侧 `homeserver.yaml` 的 `${TURN_SHARED_SECRET:?...}` 维持 fail-closed。

**C5｜联邦签名允许明文 & master key 可空** — 中｜✅ 已修复（2026-10-04）
- 原证据：`docker/config/homeserver.yaml:143-144`（`allow_plaintext_signing_keys: true`）
- 修复：部署配置改为 `allow_plaintext_signing_keys: false`，与代码默认一致，恢复 **fail-closed**——无主密钥时不再回退明文持久化，而是拒绝写入（`KeyRotationManager::resolve_stored_secret_key` 三分支中「无主密钥 + 无 opt-in」分支）。标准部署 `docker-compose.yml:247` 已强制 master key 非空（`FEDERATION_MASTER_KEY:?`），故常规部署零破坏。

**C6｜密码重置邮箱端点缺专门限流** — 低｜✅ 判断失效
- 说明：复核发现该端点已被既有前缀限流规则覆盖，原结论不成立。

**C7｜SAML 响应签名验证可被静默跳过（fail-open）** — 高｜✅ 已修复（2026-10-04）
- 原证据：`synapse-services/src/saml_service.rs:668-690`（`verify_saml_signature` 返回「No IdP metadata / No IdP certificate」时仅 `debug!` 记录并**继续放行**）
- 影响：当 IdP 元数据/证书在运行期不可用（拉取失败、缓存未就绪）时，**未验签的 SAML 断言可被接受** → 认证绕过
- 修复：`validate_response` 改为 fail-closed —— `verify_saml_signature` 任意失败（含元数据/证书不可用）均 `return Err(ApiError::unauthorized(...))` 拒绝登录；同步将原依赖 fail-open 的单测 `test_validate_response_accepts_valid_constraints` 改写为 `test_validate_response_rejects_when_signature_unverifiable`（断言拒绝语义）；`cargo test -p synapse-services --all-features saml_service` 13/13 通过，`cargo clippy --workspace --all-targets --all-features -- -D warnings` 全绿

**C8｜SAML `InResponseTo` 在无 `relay_state` 时跳过校验** — 中｜✅ 已修复（2026-10-04）
- 原证据：`synapse-services/src/saml_service.rs`（`expected_in_response_to` 为 `None` 时不比对）
- 影响：放宽重放保护
- 修复：`validate_response` 改为**无条件强制**——`expected_in_response_to` 缺失即 `Err(ApiError::unauthorized(...))` 拒绝（fail-closed），不再因 `relay_state == None` 跳过比对；`get_auth_redirect` 在调用方未提供 relay_state 时由服务端生成随机值（`saml_<uuid>`）并持久化 pending request，保证合法登录路径仍可关联。新增单测 `test_validate_response_rejects_missing_pending_request`（`cargo test -p synapse-services --all-features saml_service` 14/14 通过）。

**C9｜管理员 MFA 默认关闭** — 中｜✅ 已修复（2026-10-04）
- 原证据：`synapse-common/src/config/security.rs`；`docker/config/homeserver.yaml` 未开启
- 影响：管理员账户仅凭口令即可登录高权限面
- 修复：**部署侧默认开启**（secure-by-default）。`admin_mfa_required` 为 bool 字段，无法用 homeserver.yaml 的 `${VAR}` 插值，改由部署 `docker/deploy/docker-compose.yml` 注入 `SYNAPSE__SECURITY__ADMIN_MFA_REQUIRED`（默认 `true`，config 0.14 会将字符串强制转为 bool）覆盖共享 yaml；`ADMIN_MFA_SHARED_SECRET` 由 `:?` 强制非空（缺失即 docker compose 拒绝启动），`validate()` 对「required=true 但 secret 空」fail-closed。`docker/deploy/.env.example` 与 `scripts/generate-secrets.sh`（新增 `admin-mfa` 子命令）补齐 base32 TOTP 密钥生成。共享的 `docker/config/homeserver.yaml` 保持默认关闭以免影响 dev 栈。新增守卫测试 `env_override_coerces_string_to_bool_for_admin_mfa`。

**C10｜管理员角色 `user_type` 缺失时静默降级为 `"admin"`** — 低-中｜✅ 已修复（2026-10-04）
- 原证据：`synapse-web/src/utils/admin_auth.rs:165-174`
- 影响：缺失/空白 `user_type` 时按全权限 `admin` 处理，违反最小权限
- 修复：`normalize_admin_role` 缺失或空白 `user_type` 时返回哨兵值 `NO_ADMIN_ROLE = "none"`（[admin_auth.rs:169](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L169)、[L182](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L182)），命中 `is_role_allowed` 的 `_ => false` 兜底分支 → **fail-closed 按最小权限拒绝**，并打 `security_audit` 告警。新增 2 单测（缺省/空白均归为 `NO_ADMIN_ROLE`）。`cargo test -p synapse-web --lib --features test-utils utils::admin_auth` 12/12 通过。

**C11｜SAML XML 使用字符串/正则解析** — 低-中｜✅ 已修复（2026-10-04）
- 原证据：`synapse-services/src/saml_service.rs`（`audience_re`/`status_code_re`/`response_destination_re`/`subject_confirmation_recipient_re` 等 cached_regex + `ATTRIBUTE_VALUE_FALLBACK`）
- 影响：字符串/正则提取对畸形、命名空间前缀变化、属性顺序变化、CDATA 等形态脆弱，解析健壮性风险
- 修复：在 `synapse-common::xml_parser` 新增 `SamlResponseEnvelope`（[xml_parser.rs:344](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/xml_parser.rs#L344)）与基于 `quick_xml::Reader` 的 `parse_saml_response_envelope`（[xml_parser.rs:211](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/xml_parser.rs#L211)），`saml_service.rs` 的 7 个提取函数（`extract_in_response_to`/时间窗/audience/status/destination/recipient/issuer）与 4 个 `validate_*`、1 处调用点全部改由该解析器驱动；删除 4 个失效正则、fallback 正则及其构造器。命名空间无关读取（`local_name()`）保证跨前缀/顺序稳定；**行为严格化**——畸形 XML 由「静默降级为字段缺失」改为显式 `ApiError::bad_request`（fail-closed）。XMLDSig 层（`extract_signature_value`/`canonicalize_xml` 等）保留不动（c14n 为独立关注点）。新增 4 单测（完整/LogoutResponse/顺序无关/空输入）。`cargo test -p synapse-common --lib xml_parser` 16/16、`cargo test -p synapse-services --lib --features test-utils,saml-sso saml_service` 14/14 通过，clippy 干净。

**C12｜CORS `* + credentials` 仅告警不拒绝** — 低｜✅ 已修复（2026-10-04）
- 原证据：`synapse-common/src/config/validation.rs:92-98`
- 影响：`allowed_origins=["*"]` 搭配 `allow_credentials=true` 是浏览器拒绝的不安全组合，配置期仅 `warn!` 记录而放行
- 修复：`validate()` 改为 **fail-fast `return Err(...)`**（[validation.rs:97-101](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/config/validation.rs#L97-L101)），错误信息指明需改用显式 origin 或关闭 `allow_credentials`。新增 3 单测（通配+凭据拒绝、通配无凭据放行、显式 origin+凭据放行）。`cargo test -p synapse-common --lib config::validation` 22/22 通过。

**C13｜`panic="abort"` 提升进程级 DoS 面** — 低｜✅ 已修复（2026-10-04）
- 原证据：`Cargo.toml`（`[profile.release] panic = "abort"`）
- 说明：abort 下单个 panic 终止整个进程（无 unwinding 隔离），放大 DoS 面并阻碍优雅降级
- 修复：`[profile.release]` 改为 `panic = "unwind"`（[Cargo.toml:179](file:///Users/ljf/Desktop/hu_ts/synapse-rust/Cargo.toml#L179)），单个连接/任务的 panic 不再拖垮整个进程。

**C14｜待确认安全项** — 低｜✅ 已核实（2026-10-04）
- **VAL-01｜JSON 请求深度限制** — ✅ 已具备内置缓解：`serde_json` 仅在 `unbounded_depth` feature 关闭时保留默认递归上限（128）。经 `cargo tree -e features -i serde_json` 实测，依赖图**只启用** `default`/`raw_value`/`std`，**未启用** `unbounded_depth`，故所有 `serde_json` 反序列化路径（含 axum `Json` 与自定义 `MatrixJson` 提取器）均受 128 层深度保护，深层嵌套 DoS 已被拦在 `M_BAD_JSON`/`M_NOT_JSON` 之外。**无需改动**；后续若引入 `unbounded_depth` 需重新评估。
- **DEP-01｜`cargo audit` CI 常态化** — ✅ 已满足：`.github/workflows/ci.yml` 存在 `security-audit` job（L983-1022），安装 `cargo-audit` 并执行供应链门禁（`cargo-deny` + `cargo-audit`），依赖漏洞扫描已纳入 CI。**无需改动**。
- **Abuse-01｜无全局 IP 黑名单** — ✅ 已核实（**确认为设计取舍，非缺陷**）：经全仓检索，**不存在**请求级/全局客户端 IP 拒绝名单中间件；现有 IP 相关防护均为**定向用途**而非全局黑名单：(1) SSRF 目标黑名单 `url_preview.ip_range_blacklist` + `security::ssrf_blacklist()`（[security.rs:229](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/security.rs#L229)、[L314](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/security.rs#L314)）；(2) 推送网关 SSRF/环回防护 `is_blocked_ip`（`synapse-services/src/push/gateway.rs`）；(3) 房间级 `block/unblock`（非客户端 IP）；(4) 邀请黑名单 `invite_blocklist`（房间/用户级，非 IP）。客户端 IP 拒绝按生产惯例**下沉至反向代理/防火墙**（nginx `deny`、fail2ban、云 WAF），应用层不重复实现。注：`project_rules.md §11.1` 所列「IP Blocking ✅」实际指上述定向防护（SSRF/IP 段黑名单），非全局客户端 IP 黑名单，措辞易生歧义，建议后续在规则文件中加注澄清。

> 正面：SQLi/路径穿越/命令注入/SSRF/TLS 校验/AEAD/JWT/refresh 轮换/IDOR 经复核均无问题。

### D. 兼容性与可访问性

**D1｜SDK 错误文档格式不符** — 中｜✅ 已修复（格式部分）
- 证据：`docs/sdk/errors.md:23-40` 已改为 `{errcode, error, retry_after_ms?}`，与 `synapse-common/src/error.rs:930-937` 一致。
- 残留：错误码表仍含幻影码，见 **D8**。

**D2｜SDK README 版本/前缀陈旧** — 中｜✅ 已修复
**D3｜`event_id.rs` 头注释与实现矛盾** — 中｜✅ 已修复
**D4｜撤销级联口径冲突** — 低｜✅ 已修复

**D5｜`/versions` 声明 legacy r0 但服务器无 r0 路由** — 中｜✅ 已证伪（证据过时，2026-10-04）
- 原证据：`synapse-services/src/capability_governance.rs:84-86` 声明 `r0.5.0/r0.6.0/r0.6.1`
- 复核：`capability_governance.rs` 的 `CLIENT_API_VERSION_SUPPORT`（现 L62-87）仅声明 `stable("v1.1")`…`stable("v1.14")`，doc 注释明确 `The legacy r0.x versions are intentionally NOT advertised`，全库无 `r0.x` 声明。原指控不可复现，`/versions` 声明与实际 v3 服务面一致。

**D6｜SAML 默认 ACS/SLS URL 指向不存在的 r0 路径** — 中｜✅ 已证伪（证据过时，2026-10-04）
- 原证据：`synapse-common/src/config/auth.rs:322,327`（`/_matrix/client/r0/login/sso/redirect/saml`、`/_matrix/client/r0/logout/saml`）；测试 `:435,451` 固化了错误值
- 复核：`auth.rs` 现文（L319-329）`get_sp_acs_url`/`get_sp_sls_url` 默认均为 `/_matrix/client/v3/...`；测试（L431-453）亦断言 v3。原指控不可复现，无 r0 默认残留。

**D7｜`m.room_versions.available` 仅含 `"12"`** — 中｜✅ 已证伪（证据过时，2026-10-04）
- 原证据：`synapse-common/src/room_versions.rs:206-219`
- 复核：`client_room_versions_capability()`（现 L206-229）遍历 `SUPPORTED_ROOM_VERSIONS`（v1–v12）逐项写入 `available`，`default = DEFAULT_ROOM_VERSION = "12"`；契约快照 `capabilities_v3.snap` 实证 `available` 含 `"1".."12"` 全部 12 个版本。原指控不可复现。

**D8｜`docs/sdk/errors.md` 错误码表含不存在的 errcode** — 中｜✅ 已修复（2026-10-04）
- 原证据：`docs/sdk/errors.md:91` 列出 `M_INVALID_PASSWORD` 等；全仓 `synapse-common/src` 无该 errcode 定义
- 复核/处置：`errors.md` 已重写（L85-95），声明唯一权威来源为 `synapse-common/src/error/code.rs` 的 `MatrixErrorCode`，并显式声明 `M_INVALID_PASSWORD`/`M_USER_NOT_FOUND` 等历史幻影码「均不存在」；错误码表已与 `code.rs` 全部 43 个变体逐条对齐。

**D9｜`/versions` 上限 v1.14 vs 文档声称 v1.19** — 中｜✅ 已订正（2026-10-04）
- 处置：与 D14 同源，属「上游最新发布规范」与「本仓声明基线」两个概念被混用。`AGENTS.md` §Current external baselines（现 L449-451）已拆分为「上游跟踪目标 v1.19（随上游移动，非实现声明）」与「本仓声明基线 v1.1–v1.14（单点权威 = `capability_governance.rs` 的 `CLIENT_API_VERSION_SUPPORT`）」，并显式禁止混淆。

**D10｜全站缺 i18n（`ui_locales_supported: ["en"]`）** — 中｜✅ 已修复（2026-10-04）
- 影响：非英语用户可访问性受限
- 处置：采取「明确立场并文档化」路径 —— `docs/sdk/README.md`（L148-155）新增「国际化（i18n）」章节，显式声明当前仅支持英文 `en`、与 `ui_locales_supported: ["en"]` 一致，并说明多语言属后续 backlog；不再构成未声明的口径漂移。

**D11｜健康探针不完整，`/healthz` 为死配置** — 低-中｜✅ 已证伪（证据过时，2026-10-04）
- 原证据：`synapse-web/src/routes/http_metrics.rs:31`
- 复核：真实文件为 `synapse-web/src/middleware/http_metrics.rs`，其 `EXCLUDED_PATHS = &["/health", "/metrics"]`，**不含 `/healthz`**；且 `/health` 为真实路由（`synapse-web/src/routes/assembly.rs:181`）。原指控不可复现，健康探针无死配置。

**D12｜迁移回滚文档陈旧** — 低｜✅ 已修复（2026-10-04）
- 原证据：`migrations/README.md:296-305` 指引不存在的 `.undo.sql`
- 复核/处置：属**过时证据**——工作区 `migrations/README.md`（现 L295-310）已改为真实回滚机制：明确「本目录**不存在** `.undo.sql`」，并给出 dev 重建（幂等 baseline 重跑）/ 生产 **forward-fix** 两条路径，末行显式警示不要执行不存在的 `.undo.sql`。与 `docker/db_migrate.sh`、`scripts/check_migration_consistency.py`（`.undo.sql` 仅作排除/历史语义）一致。

**D13｜`ELEMENT_SYNAPSE_GAP_ANALYSIS` 房间版本失实** — 中｜✅ 已修复（2026-10-04）
- 原证据：文档称 v1–v13、默认 v10
- 复核/处置：`ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md:27` 现为「v1–v12（默认 v12）」对标上游「v1–v12（默认 v10）」，全库无 `v13`；文档头部已注明其为历史快照及现行基线入口。与 `room_versions.rs`（`DEFAULT_ROOM_VERSION="12"`、`SUPPORTED_ROOM_VERSIONS`=v1–v12）一致。

**D14｜规范基线口径冲突（v1.19 vs v1.18）** — 低｜✅ 已订正（2026-10-04）
- 根因：文档混用两个不同概念——**上游最新发布规范**（`AGENTS.md:450` 的 v1.19）与**本仓声明基线**（`/versions` 实际上限 v1.14），且无单点权威，致 v1.18/v1.19/v1.14 三个数字并存。
- 复核：唯一权威 = `synapse-services/src/capability_governance.rs` 的 `CLIENT_API_VERSION_SUPPORT`（v1.1–**v1.14**）；`API_COVERAGE_REPORT.md:12`、`ELEMENT_SYNAPSE_GAP_ANALYSIS…:6` 已对齐 v1.14。
- 处置：`AGENTS.md` §Current external baselines 拆分为「上游跟踪目标 v1.19（随上游移动，非实现声明）」与「本仓声明基线 v1.1–v1.14（单点权威 = `CLIENT_API_VERSION_SUPPORT`）」两条，并显式禁止混淆；`CHANGELOG` 及 `docs/audit/*_2026-09-*` 等**带日期快照**按时间点留档（历史基线，不回改），单点维护口径自此确立。

> 正面：错误响应符合 Matrix 规范、安全头齐全、ACL fail-closed、v1/v2 与 v3+ event_id 处理正确。

### E. 代码质量与技术债务

> E1–E15 的逐条落地核验见 §八。

| 编号 | 状态 | 问题 | 严重度 | 证据 |
|------|------|------|--------|------|
| **E1** | ✅ 已处置 | clippy-allow 属性实测 **264 处**（外层 `#[allow(clippy::…)]` 152 + 内层 `#![allow(clippy::…)]` 112；原述「≈160」仅计外层、**低估**）。内层 112 全为测试/测试模块**文件级豁免**（在 `unwrap_used/expect_used/panic = deny` 门下为 load-bearing、既定模式）；外层 152 为真实重构目标，本轮就地消除 9 处冗余 allow | 中｜**P2** | 全仓；核验/归档/批次计划见 §八 |
| E2 | ✅ 已修复 | `create_event` ≈194 行超长函数 | 低-中｜P3 | `synapse-services/src/room/messaging/events.rs` |
| E3 | ✅ 已修复 | `src/e2ee/`+`src/cache/` 多层 facade 冗余 | 低-中｜P3 | 已折叠为直接 re-export |
| E4 | ✅ 归档 | `vendor/pastey` fork 不受门禁 | 低｜P3 | `vendor/`（by-design，RUSTSEC-2024-0436 缓解） |
| E5 | ✅ 归档 | 债务标记/未实现/unwrap 计数表述不准 | 低｜P3 | 生产实质缺口 0 |
| E6 | ✅ 归档 | `.clippy.toml` 阈值「放宽」表述不准 | 低｜P3 | `.clippy.toml` |
| **E7** | ✅ 已处置 | `.clippy.toml` 的 cognitive-complexity/too-many-lines/single-char 三项**完全惰性**（pedantic/nursery lint 组从未启用；CI `.github/workflows/ci.yml:353` 无 `-W clippy::pedantic`） | 中｜**P2** | `.clippy.toml` 已加注释说明「阈值仅被 pedantic 消费、当前不强制」，明确保留意图 |
| **E8** | ✅ 已处置 | 约 30 处生产代码用 `#[allow(clippy::unwrap_used/expect_used)]` 逃逸 deny 门禁 | 中｜**P2** | 删除 12 处（`user_service.rs` 8 处改毒化容错锁、e2ee 存储 3 处改 `UNIX_EPOCH`、`wiring/e2ee.rs` 1 处冗余）；剩余约 19 处生产（另 tests/benches/test-utils 约 10 处，合计 29 处/25 文件），多为 load-bearing，转后续批次 |
| **E9** | ✅ 已处置 | `send_message_inner` ≈323 行（比已拆的 `create_event` 更长） | 中｜**P2** | `synapse-services/src/room/messaging/messages.rs` 已拆为主函数 + 4 个私有辅助（`build_beacon_location_params`/`index_relation_in_tx`/`record_txn_dedup_in_tx`/`run_post_commit_fan_out`），行为等价 |
| **E10** | ✅ 已修复 | 重复样板：`metadata.rs` 空值清理 6 段 + `hierarchy.rs` 两处逐字相同的 `room_type` 提取 | 低｜P3 | `metadata.rs`、`hierarchy.rs` |
| **E11** | ✅ 归档 | 「存储层越界」不成立：`room/mod.rs` 的**批量**删事件为 `DB-04-b` 已文档化设计（单事务分批，避开 `events` 表级 `AccessExclusiveLock`）；`event/search.rs` 的 JOIN 为**只读**搜索按已加入房间过滤，非写越界 | 低｜P3 | `room/mod.rs:887-915`、`event/search.rs:169-201` |
| **E12** | ✅ 归档 | 分页上限差异为**按端点设计**：admin 列表统一 `.clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT)`；其余端点上限随载荷而定（100–1000），统一化会改变响应规模，非缺陷 | 低｜P3 | `constants.rs:21-28` |
| **E13** | ✅ 订正 | 「生产 ≈3 处」不成立：17 处 `#[allow(dead_code)]` 中，`aes.rs`/`voice.rs` 全部由 `#[cfg(test)]` 门控（测试 helper），仅 `service.rs:33` 为 `#[cfg(not(feature="beacons"))]` 特性门控占位字段（有意为之、构建期必需）；无生产逃逸 | 低｜P3 | 全仓（17 处） |
| **E14** | ✅ 归档 | `#[ignore]` 全在 `tests/`，各有 reason（e2e 需 `E2E_RUN=1`+homeserver、perf 手工冒烟、bench），且由 `e2e_honesty_tests.rs` 守卫「禁止裸 `#[ignore]`」；非生产债务 | 低｜P3 | `tests/` |
| **E15** | ✅ 归档 | 动态 SQL `format!` 仅拼接**静态片段**（列名常量/固定游标子句/占位符序号），值全部 `bind`；生产动态查询受 `sqlx_dynamic_literal_guard_tests.rs` 守卫与 ratchet 管控，其余 9 文件多为测试基础设施 | 低｜P3 | `pagination.rs`、`sqlx_dynamic_literal_guard_tests.rs` |

### F. 文档与知识库

| 编号 | 状态 | 问题 | 严重度 | 证据 |
|------|------|------|--------|------|
| **F1** | ✅ 已修复 | 路径漂移：`src/web`/`src/services`/`src/e2ee` → 实为 `synapse-web/src`/`synapse-services/src`/`synapse-e2ee/src` | 高｜**P1** | **订正**：报告原列 4 处证据（`CONTRIBUTING.md:70-72`、`AGENTS.md:400`、`docs/INDEX.md:5,26,40`、`sdk-encapsulation-audit.md:472,488,542`）经复核**均已正确**、属过时指控；真实漂移在现行文档，已改：`LEDGER_EXPORT_SCHEMA.md:3,75`、`MSC_SEMANTICS.md:20-21`、`ROUTE_CONTRACT.md:1551`、`ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md:374`、`api-error.md:12`、`trigram-audit.md:126`、`quality/{LOGGING_ENHANCEMENT,PERMISSION_ANALYSIS,API_ENDPOINTS_STATUS}.md`；2026-09 迁移前的历史审计快照与 `archive/` 保留原样 |
| F2 | ✅ 已修复 | `docs/INDEX.md:78-82` 引用不存在的 `NN_*.md` | 高｜P1 | 已清理 |
| F3 | ✅ 已修复 | 两套监控栈文档互斥指令 | 高｜P1 | — |
| F4 | ✅ 已修复 | 死链：README/TESTING/CONTRIBUTING | 中｜P2 | — |
| F5 | ✅ 已修复 | 迁移基线口径冲突（CHANGELOG v10 vs v12） | 高｜P1 | — |
| F6 | ✅ 已修复 | 文件名与内容不符（`overview.md`、`QUICKSTART.md`） | 中｜P2 | — |
| F7 | ✅ 已修复 | `docker/deploy/README.md` 漏选项、流程不符 | 中｜P2 | — |
| F8 | ✅ 已归档 | 文档噪音：`docs/audit/*.md`=46、`docs/archive/**/*.md`=58、`docs/superpowers/plans/*.md`=36 | 低｜P3 | `docs/INDEX.md §六/§七` 已为三处目录提供入口，并声明「历史/一次性报告统一进入 `archive/`，仅溯源、不再作契约引用」「归档文档只读、不再维护」，残留为 **by-design 历史留存**；本轮补入 `superpowers/plans/` 索引项 |
| **F9** | ✅ 已修复 | `docs/INDEX.md:32` 称 `LEDGER_EXPORT_SCHEMA.md` **不存在**，实际存在于 `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md` | 高｜**P1** | 已从「均已不存在」列表移除，并在 §二 契约索引补入正向条目 |
| **F10** | ✅ 已修复 | `docs/synapse-rust/archive/README.md:9,12,19` 权威来源指向不存在文件（含 4 处死链），且与 `INDEX.md` 自相矛盾 | 高｜**P1** | 已改为指向现行 `docs/INDEX.md`，删除失效文件名（替代文件确不存在，内容已融入现行文档） |
| **F11** | ✅ 已修复 | `MSC_SEMANTICS.md` 记 MSC3912「客户端撤回不级联」，与代码矛盾（`events.rs` 在 `with_rel_types` 非空时**会**级联） | 高｜**P1** | 已订正为「单层级联、不递归、不触碰父事件」并补入代码证据 `events.rs:1134-1169` |
| **F12** | ✅ 已修复 | `docs/sdk/` 除 README 外 111 处使用 `r0` 前缀，与代码 v3 不符 | 高｜**P1** | 已批量替换为 v3（e2ee 17、rooms 38、authentication 27、messages 29），`/_matrix/client/r0/` 零残留 |
| **F13** | ✅ 已修复 | `issues/` 与 `E2EE_VODOZEMAC_MIGRATION.md` 多处死链 | 中｜**P2** | **部分证伪 + 修复**：`E2EE_VODOZEMAC_MIGRATION.md` 全部相对链接（含 `src/e2ee/mod.rs`、`synapse-e2ee/src/...`、`Cargo.toml`、`../../sdk/e2ee.md`）解析目标均存在，无死链；真实死链仅在 `issues/M3-ISSUE-1`、`M3-ISSUE-2` 的 `Origin` 行指向已删除的 `M3_BATCH1_EXECUTION_PLAN.md`（全库不存在），已改为纯文本并注明「原始计划文档已不再保留」，与 `issues/README.md` 口径一致 |
| F14 | ✅ 已证伪 | `DEPENDENCY_UPGRADE_TRACKER.md` 与 `project_rules.md §17.5` 口径冲突（9 组 vs 11 组） | 中｜**P2** | 证据过时：两文件现均为 **38 组**（tracker 头部/§4 与 rules §17.5 摘要一致），且 rules §17.5 已声明以 tracker 为「唯一权威口径」、不再重复维护明细 |
| F15 | ✅ 已证伪 | `project_rules.md §16` 引用的 3 个文档均不存在 | 中｜**P2** | 证据过时：§16（L654-659）现有 **6 个**引用（`docs/INDEX.md`、`ROUTE_CONTRACT.md`、`API_COVERAGE_REPORT.md`、`migrations/README.md`、`migrations/INDEXES.md`、本审计报告），经逐一确认**全部存在** |
| F16 | ✅ 已修复 | `openapi/README.md` 数字自相矛盾（898 vs 1096） | 中｜**P2** | 已澄清：`openapi/README.md`（L87-91）新增显式警告「两个计数口径不可混用」——`ledger.json` 全量 = **1096** 端点、`client.yaml` default profile = **898** operations，并说明二者统计口径差异 |
| F17 | ✅ 已修复 | `docs/sdk/README.md` 测试域名 `cjystx.top` vs 部署口径 `matrix.test` | 中｜**P2** | 已改：`docs/sdk/README.md:11` 测试域名现为 `matrix.test`，SDK 文档无 `cjystx.top` 残留 |
| F18 | ✅ 已修复 | 上游基线口径冲突（INDEX 称 gap 分析为现行基线 v1.162 vs API_COVERAGE_REPORT 称其 v1.156 已落后） | 中｜**P2** | 已对齐：`docs/INDEX.md:27-28` 标注 gap 分析为「历史快照，基线 v1.156.0；现行上游基线 v1.162.0 见 API_COVERAGE_REPORT.md」，与 `API_COVERAGE_REPORT.md:12`（对齐 element-hq/synapse v1.162.0）一致 |
| F19 | ✅ 归档 | 归档文档残留 r0/旧路径（实测：`_matrix/client/r0` 71 处/8 文件、`src/{web,services,e2ee,cache}/` 旧路径 434 处/29 文件，全在 `docs/archive/**`，含一处 `.trae_openapi_missing.txt` 快照） | 低｜P3 | `docs/INDEX.md §七` 已声明归档**只读、不再维护**；历史快照按当时布局保留、回改将曲解溯源；**现行文档**的 r0/路径漂移已由 F1/F12 纠偏（`docs/sdk/` 等已零残留） |
| F20 | ✅ 已澄清 | `CHANGELOG.md` 旧路径（9 处 `src/{web,services,e2ee,cache}/`） | 低｜P3 | 已在 `CHANGELOG.md` 头部补注：2026-09 模块化拆分前的历史条目沿用当时单 crate 路径、不回改；现行路径为 `synapse-{web,services,e2ee}/src/` |
| F21 | ✅ 归档 | 缺失现行安全/E2EE/联邦/存储 schema 文档 | 低｜P3 | 现行覆盖已分散于既有索引文档（非缺失）：E2EE `docs/sdk/e2ee.md`；安全 `docs/security/ci-security-grading.md`+`docs/monitoring/{network,nginx}-security.md`；联邦 `docs/templates/federation-edu-persist-template.md`+`MSC_SEMANTICS.md`；存储 schema `migrations/README.md`+`migrations/INDEXES.md`+`docs/superpowers/STORAGE_MIGRATION_MAP.md`。合并式专文属 nice-to-have backlog，非缺陷 |

---

## 五、本轮复核：首轮清单处置台账

| 处置 | 数量 | 编号 |
|------|------|------|
| ✅ 已修复/归档 | 36 | A1、A2、A3、A4、**A5**、B1、B2、B3、**B4**、**B5**、B6、**B7**、C1、C2、C3、**C4**、C5、C6、D1、D2、D3、D4、**E1**、E2、E3、E4、E5、E6、F1、F2、F3、F4、F5、F6、F7、**F8** |
| ⚠️ 仍存续 | 0 | — |

> 说明：首轮头部计数（31）与正文条目（36）本身不一致，本轮以正文条目为准重算（末次更新：**36 已处置 + 0 仍存续 = 36，全部清零**）。其中 `F1`（文档路径漂移）已于 2026-10-04 完成现行文档纠偏，故由「仍存续」转入「已修复/归档」；`A3`（voice 三端点）已于同日随 P2 代码批次处置，同样转入「已修复/归档」；`C5`（联邦签名明文）已于同日随 P2 安全配置批次处置，转入「已修复/归档」；`A5`（admin 媒体 `users`/`user` 单复数）经核对上游后确认**忠实镜像上游、非缺陷**，同日澄清关闭，转入「已修复/归档」；`B4`（全表 COUNT，`cfg(test)` 门控）、`B5`（EDU Semaphore 驱逐上限）、`B7`（`spawn_blocking` 化）已于同日随 P3 内存/性能批次处置，转入「已修复/归档」；`F8`（文档噪音「部分缓解」）已于同日随 P3 文档批次处置（`docs/INDEX.md` 覆盖三处目录 + 归档只读声明 + 补 `superpowers/plans/` 索引项），转入「已修复/归档」。末两项 `C4`（部署示例弱默认 `TURN_SHARED_SECRET=dev-turn-secret`）已于同日处置完成（占位符化 + `deploy.sh` fail-closed + 停止回显密钥，`dev-turn-secret` 全仓清零），`E1`（clippy-allow 属性）已于同日完成核验与有限消除（实测 **264 处** = 外层 152 + 内层 112，就地消除 9 处冗余 outer allow + 2 处 token 级 `useless_conversion`，其余 load-bearing 逐类归档并给出重构批次计划，详见 §八），二者同日转入「已修复/归档」。

---

## 六、重要订正：既有审计文档的失效结论

本次复核**证伪**了仓库内多份既有文档的结论，使用它们时须警惕：

| 既有结论 | 实际（本次证据） |
|----------|------------------|
| Content Scanner「空转」 | 已接线：`synapse-web/src/routes/media/upload.rs:89,137`、`synapse-web/src/routes/handlers/room/events.rs:309-317` |
| 缩略图 `animated`「MISSING」 | 已全链路实现：`synapse-services/src/media_service.rs:492-499,549-574,630-717` |
| MSC3912 客户端撤回「不级联」 | `with_rel_types` 非空时**会**级联（`synapse-web/src/routes/handlers/room/events.rs:1134-1169` + `synapse-services/src/event_redaction_service.rs:120-192`） |
| P0-2 event_id reference hash「待解决」 | 已解决 |
| 本报告 §四 D/F 表 13 项 P2（`D5–D10/D13`、`F13–F18`）指控 r0 残留 / 幻影 errcode / 版本口径 / 死链 | 逐项取证后：**5 项证据过时**（`D5/D6/D7/F14/F15`，指控不可复现）、**6 项已修复/澄清**（`D8/D10/D13/F16/F17/F18`）、**1 项订正**（`D9`，与 D14 同源）、**1 项部分真实**（`F13`：仅 `issues/` 两处 `Origin` 死链，已修复；`E2EE_VODOZEMAC_MIGRATION.md` 链接全部有效）——报告首轮所列多为**取证时点滞后**所致 |

这本身即一项**文档可信度风险（P1 级）**：`docs/audit/`、`docs/INDEX.md`、`docs/sdk/` 多处结论与代码漂移，建议在报告签发时统一标注失效（对应 F9、F10、F11、F12）。

---

## 七、风险分析与建议处置顺序

**风险画像**
- **可利用安全风险（最高）**：`C7` SAML fail-open 曾是本轮唯一 P1 安全项（在「元数据拉取失败」运行态即触发认证绕过），**已于 2026-10-04 修复为 fail-closed**；配置/降级类安全项 `C5/C8/C9` 亦于同日随 P2 安全配置批次修复（联邦签名恢复 fail-closed、SAML `InResponseTo` 无条件强校验、部署侧默认开启管理员 MFA），**当前无未处置的认证降级/绕过类安全项**；`C4` 弱默认残留（`TURN_SHARED_SECRET=dev-turn-secret`）属部署示例噪音（非运行时绕过），**已于 2026-10-04 处置**：去除弱默认、统一为占位符，并在 `deploy.sh` 对未设置/占位符显式报错。
- **内存单调增长**：`B1/B8/B9/B11/B12/B13/B14` 均为「无上限容器 + 长跑实例」模式（OOM 风险主要来源），已按同一治理模板（容量上限 + 淘汰/清理策略）处置；`B4`（全表 COUNT）、`B7`（async 内阻塞 IO）、`B15`（阻塞 DNS）分别以 `cfg(test)` 门控 / `spawn_blocking` 处置；`B10`（只写不读缓存）整体移除；`B16/B17` 经核实分别有界、test-only，非缺陷。
- **文档可信度**：`F9–F12` 属「索引自相矛盾 / 与代码不符」，会直接误导开发者与 SDK 使用者，且 CI 死链门禁可能因此报红。
- **兼容性**：`D5/D6/D8/D13` 曾反映 r0 残留与版本/错误码口径漂移；经 2026-10-04 逐项取证，`D5/D6` 属证据过时（无 r0 声明/r0 默认）、`D8` 已重写错误码表并以 `error/code.rs` 为唯一权威、`D13` 已订正为 v1–v12 / 默认 v12，**兼容性口径已全部对齐**。

**建议顺序**
1. **立即（P1，低成本高收益）**：~~`C7` SAML 改 fail-closed~~ ✅ 已完成（2026-10-04）；~~`F1` 路径漂移~~、~~`F9/F10/F11/F12` 文档纠偏~~ ✅ 均已处理（2026-10-04）——**P1 全部清零**
2. **本迭代（P2 代码）** ✅ 已完成（2026-10-04）：~~`A6/A7` 功能接线与错误体归一化、`A3` voice 端点、`B8/B9/B12/B13/B14` 无界容器治理、`E7/E8/E9` lint 门禁与超长函数~~ —— A6 按口径校正归档（未改代码）、A7 错误体归一化（裸 `Json` 迁移遗留）、A3 能力声明（不改 501 行为）、B8/B9/B12/B13/B14 有界化、E7 惰性阈值文档化、E8 删除 12 处逃逸 allow、E9 拆分完成
3. **安全加固（P2 配置）** ✅ 已完成（2026-10-04）：~~`C5/C8/C9` 密钥与 SAML/MFA 配置~~ —— C5 部署 yaml 明文签名开关改 `false`（恢复 fail-closed）、C8 `validate_response` 无条件强校验 `InResponseTo` + `get_auth_redirect` 服务端生成 relay_state、C9 部署侧默认开启管理员 MFA（env 覆盖 bool + `:?` 强制 secret 非空 + `generate-secrets.sh admin-mfa`），配套新增 SAML 拒绝单测与 config env→bool 守卫测试；`C4` 残留弱默认（`TURN_SHARED_SECRET=dev-turn-secret`）已于同日处置完成 —— 部署示例占位符化（`__CHANGE_ME_match_coturn_static_auth_secret__`）+ `deploy.sh` 对未设置/占位符 `log_error` fail-closed + 停止回显密钥，`dev-turn-secret` 全仓清零
4. **兼容性对齐（P2 文档/契约）** ✅ 已完成（2026-10-04）：~~`D5–D10/D13`、`F13–F18`~~ —— `D5/D6/D7` 证伪（无 r0 声明 / SAML 默认已 v3 / `available` 含全量 v1–v12）、`D8/D10/D13` 修复（错误码表重写 / i18n 立场文档化 / gap 分析改 v1–v12）、`D9` 订正（与 D14 同源，单点权威）、`F13` 修复（`issues/` 两处 `Origin` 死链改纯文本，E2EE 归档文经证伪无死链）、`F14/F15` 证伪（38 组口径一致 / §16 六引用全部存在）、`F16/F17/F18` 修复或澄清（openapi 两口径已澄清 / 测试域名 `matrix.test` / INDEX 与 API_COVERAGE 对齐 v1.162）
5. **收尾（P3）**：其余功能缺口、次要性能、配置、注释与文档噪音（A5/A8–A14、B4/B5/B7/B10/B11/B15–B17、C10–C14、D11/D12/D14、E10–E15、F8/F19–F21）
   - 进行中（2026-10-04）：~~`C10/C11/C12/C13`（修复）~~、~~`C14`（VAL-01/DEP-01 已满足、Abuse-01 设计取舍）~~、~~`A9/A10/A12/A13/A14`（行为严格化：显式报错）~~、~~`B4/B5/B7/B10/B11/B15`（内存/阻塞治理修复）~~、~~`B16/B17`（核实：有界/test-only 非缺陷）~~、~~`D11`（证伪：证据过时）~~、~~`E10–E15`（E10 修复；E11/E12/E14/E15 归档；E13 订正）~~、~~`F8/F19–F21`（F8/F19/F21 归档；F20 澄清）~~、~~`E1`（有限消除 + 精确归档：264 处口径与重构批次计划见 §八）~~、~~`D12`（修复：迁移回滚文档改为真实机制）~~、~~`D14`（订正：拆分上游跟踪目标 v1.19 与本仓声明基线 v1.14，确立单点权威）~~ ✅ 已完成；**P3 全部清零（34/34）**。

---

## 八、E 系列债务核验与处置记录（2026-10-04）

> 本节为 §四 E 表的落地结论，均已对照代码与 lint 配置取证。

| 编号 | 报告原述 | 核验结论 | 处置 |
|------|----------|----------|------|
| **E1** | `#[allow(clippy::)]` ≈160 处（含 61 处 `too_many_arguments`） | **计数已订正**：原「≈160」**仅计外层** `#[allow(clippy::…)]`，漏计内层文件级 `#![allow(clippy::…)]`。以 `allow(clippy::` 实测全仓 **274 处**（含 vendored `vendor/pastey/src/lib.rs:1`）→ 项目源码 **273 处**；按属性形态拆为 **外层 `#[allow(clippy::…)]` 152 + 内层 `#![allow(clippy::…)]` 112 = 264 处属性站点**（余 9 处为 `cfg_attr`/跨行形态）。**内层 112 全为测试/测试模块的文件级豁免**（`tests/**`、`benches/`、`scripts/bench_harness.rs`、`test_mocks.rs`、部分 `src/lib.rs` 与 `*_service.rs`），模式为 `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`，在 8 个成员 crate 的 `panic = deny`（及根包 `unwrap_used/expect_used = deny`）门下为 **load-bearing、既定测试豁免**，非生产债务。**外层 152** 为真实重构目标，token 实测分布：`too_many_arguments` **62**、`type_complexity` **30**、`unused_async` **20**（全在 `synapse-web`）、`await_holding_lock` **9**、`module_inception` **4**，其余为生产 `unwrap_used/expect_used/panic` 点状豁免。token 全局实测（含内层）：`clippy::expect_used` **105**、`clippy::unwrap_used` **100**、`clippy::panic` **68**。`.clippy.toml` 的 `too-many-arguments=7` 仅对 `too_many_arguments`（complexity 组）生效，与 pedantic 惰性无关 | **有限消除**：本轮就地删除 **9 处冗余 `#[allow]` 属性（10 token）** 并消除 **1 处模块级 allow token**——`synapse-common`：`config/experimental.rs` `derivable_impls`、`argon2_config.rs` `unnecessary_wraps`、`crypto.rs` `expect_used`+`unnecessary_literal_unwrap`（改 `unreachable!`）；`synapse-e2ee`：`device_keys/storage.rs` `redundant_closure`；`synapse-test-utils`：`lib.rs` `never_loop`（改 `if let`）；`synapse-services`：`friend_room_service/mod.rs` `needless_borrow`×2；`synapse-web`：`routes/admin/register.rs` `needless_pass_by_value`（改 `&ApiError`）；根包：`src/common/error.rs` `needless_pass_by_value`（改 `&…`）；`synapse-storage`：`membership/mod.rs` 模块级去 `clippy::useless_conversion` token。**精确归档**：内层 112 处测试豁免、`module_inception`（4）、`should_implement_trait`（`error/code.rs`/`claims.rs`，改会动 API）、`redundant_clone`（`regex_cache.rs`，显式 deny 下假阳性）等 load-bearing 项记入下方**重构批次计划**，转独立迭代 |
| **E2** | `create_event` ≈194 行超长函数 | 属实 | 已拆分（主函数 + 3 个私有辅助 `normalize_redaction_placement`/`persist_event`/`run_post_create_side_effects`，行为等价） |
| **E3** | `src/e2ee/`+`src/cache/` 多层 facade 冗余 | 属实（根 crate 为纯 re-export 薄壳） | 已折叠为直接 re-export（删除 11 个薄壳 + `src/cache/mod.rs`，改写 `src/e2ee/mod.rs`、`src/lib.rs`） |
| **E4** | `vendor/pastey` fork 不受门禁 | 属实但 **by-design**：`RUSTSEC-2024-0436` 缓解，`[patch.crates-io]` 将停维护 `paste` 重定向至 pastey 0.2.3 逐字拷贝，`workspace.exclude` 排除、自带 `[lints.rust]`，源码零改动 | 归档，不处置 |
| **E5** | 债务标记 11 + 未实现 13 + unwrap/expect 887（多为测试） | 表述不准：`TODO/FIXME` 实际 6 处，全为非可执行项（1 处 vendored、1 处上游拷贝注释、4 处已解决 review TODO 的说明性 prose）；`todo!/unimplemented!` 全部位于 `#[cfg(test)]` 测试 mock 与 vendored 代码，生产实质缺口 0 | 归档，不处置 |
| **E6** | `.clippy.toml` 阈值放宽（cognitive-complexity=25 等） | 表述不准：仅 `too-many-lines-threshold=500` 相对默认(100)放宽；`cognitive-complexity=25`、`single-char-binding=4`、`too-many-arguments=7` 均为 clippy 默认值；`type-complexity=200` 相对默认(250)反而**收紧**。**补充（本轮 E7）**：即便放宽，这些阈值也因 pedantic/nursery 组未启用而惰性 | 归档，不处置；惰性问题另立 E7 |
| **E7** | `.clippy.toml` 三项阈值完全惰性 | 属实：`.clippy.toml` 的阈值仅被 `clippy::pedantic`/`nursery` 组消费，而本 workspace 从未启用这两组（`-D warnings` 不等于启用 pedantic），故认知复杂度/超长行/单字符绑定阈值完全不生效。**最小侵入**处置：不引入新 lint 组（会引爆大量存量告警、非本批范围），仅在文件头加注释说明「意图保留、当前不强制」，避免后续误判门禁强度 | 文档化，不启用 pedantic |
| **E8** | 约 30 处 `#[allow(clippy::unwrap_used/expect_used)]` 逃逸 deny 门禁 | 属实但多数 load-bearing（删除即门禁失败）。本批消除可安全改写者 12 处：`synapse-services/src/user_service.rs` 8 处（RwLock `.unwrap()` → `.unwrap_or_else(\|e\| e.into_inner())` 毒化容错）、`synapse-e2ee/src/{cross_signing,device_keys}/storage.rs` 3 处（`from_timestamp(0,0).expect(...)` → `chrono::DateTime::<Utc>::UNIX_EPOCH`）、`synapse-services/src/wiring/e2ee.rs` 1 处（冗余 `expect_used`）。剩余约 19 处生产（另 tests/benches/test-utils 约 10 处，合计 29 处/25 文件），需重构消解 | 删除 12 处；其余转后续重构批次 |
| **E9** | `send_message_inner` ≈323 行超长函数 | 属实 | 已拆分为主函数 + 4 个私有辅助（`build_beacon_location_params`[beacons 门控]、`index_relation_in_tx`、`record_txn_dedup_in_tx`、`run_post_commit_fan_out`），行为等价。注意 `sqlx::Transaction::rollback(self)` 消费所有权，故显式回滚留在持有 tx 的调用方而非 helper（已拆分） |
| **E10** | 重复样板：`metadata.rs` 空值清理 6 段 + `hierarchy.rs` 两处逐字相同的 `room_type` 提取 | 属实，但报告证据行号失准（`metadata.rs:13-47` 实为一段失准注释块；真实重复为 `metadata.rs` 内 6 段 `is_some_and(\|v\| v.is_null())` 清理、`hierarchy.rs` 两处 `m.room.create`→`content.type` 提取） | 就地提取两个模块级 helper（`metadata.rs::remove_null_keys`、`hierarchy.rs::room_type_from_state_events`），调用点等价替换；顺带删除失准注释块。`cargo clippy -p synapse-web --all-targets --all-features -- -D warnings` 退出码 0（已修复） |
| **E11** | 「存储层越界」（RoomStorage 触碰事件数据 / EventStorage 触碰成员关系） | 不成立：`room/mod.rs::delete_room` 的分批 `DELETE FROM events ... LIMIT 1000` 为 `DB-04-b` **已文档化设计**（单事务内分批，规避 `events` 表级 `AccessExclusiveLock`），非越界；`event/search.rs::search_joined_room_events` 的 `INNER JOIN room_memberships` 为**只读**搜索按已加入房间过滤，非写越界 | 归档，不处置 |
| **E12** | 分页上限差异（admin `clamp(MIN,MAX)` vs 端点 100–1000 字面量） | 不成立（按端点设计）：admin 列表统一 `.clamp(MIN_PAGINATION_LIMIT, MAX_PAGINATION_LIMIT)`；其余端点上限按载荷而定，统一化会改变响应规模，非缺陷 | 归档，不处置 |
| **E13** | 生产 ≈3 处 `#[allow(dead_code)]` | 不成立：17 处中 `aes.rs`/`voice.rs` 全部由 `#[cfg(test)]` 门控（测试 helper），仅 `messaging/service.rs:33` 为 `#[cfg(not(feature="beacons"))]` 特性门控占位字段（有意为之、构建期必需）；**无生产逃逸** | 订正，不处置 |
| **E14** | `#[ignore]` 测试债务 | 非生产债务：`#[ignore]` 全在 `tests/`，各有 reason（e2e 需 `E2E_RUN=1`+homeserver、perf 手工冒烟、bench），且由 `e2e_honesty_tests.rs` 守卫「禁止裸 `#[ignore]`」 | 归档，不处置 |
| **E15** | 动态 SQL `format!` 注入面 | 无注入面：`format!` 仅拼接**静态片段**（列名常量/固定游标子句/占位符序号），值全部 `bind`；生产动态查询受 `sqlx_dynamic_literal_guard_tests.rs` 守卫与 ratchet 管控，其余 9 文件多为测试基础设施 | 归档，不处置 |

### E1 重构批次计划（外层 152 处 load-bearing allow，转独立迭代）

> 下列批次为**逐类归档后的执行计划**，非本轮范围；每批独立成任务、逐 crate 验证（`cargo clippy -p <crate> --all-targets --all-features -- -D warnings`）。排序原则：**按消除收益/风险比**，先做低风险机械重写，再做触及 API/锁语义的改造。

| 批次 | 目标 token | 规模 | 主要位置 | 改造手法 | 风险 |
|------|-----------|------|----------|----------|------|
| **A** | `too_many_arguments` | 62 | `synapse-storage` 主导 | 引入按域命名的参数结构体（如 `CreateEventParams`）替代长参数列表；调用点以结构体字面量传入 | 低（纯签名重写、编译期校验） |
| **B** | `type_complexity` | 30 | 多在 `test_mocks/*` 与存储返回类型 | 提取 `type` 别名（如 `type EventRows = Vec<(String, i64, Vec<u8>)>`），生产类型对齐领域命名 | 低（别名不改变行为） |
| **C** | `unused_async` | 20 | 全在 `synapse-web` | 逐个判定：axum handler **必须** `async`（多为假阳性，保留并加 `#[allow]` 注释说明）；确为同步者去 `async` 并修正调用点 `.await` | 中（需区分 handler 与内部 fn，避免破坏 tower 服务契约） |
| **D** | `await_holding_lock` | 9 | `synapse-federation` `friend/client.rs` 1 + tests 8 | 缩小 `MutexGuard` 作用域：先取值/克隆再 `await`，或改用 `tokio::sync` 原语 | 中（触及并发时序，需回归联邦测试） |
| **E** | 生产 `unwrap_used`/`expect_used`/`panic` 点状豁免 | 约 20+ | 各 crate 生产路径 | 逐点改为 `?` + 领域错误、`ok_or(...)?`、毒化容错 `unwrap_or_else(\|e\| e.into_inner())`（沿用 E8 手法） | 中（错误语义需逐点核对） |
| **归档** | `module_inception`(4)、`should_implement_trait`(2)、`redundant_clone`(1)、内层 112 测试豁免 | — | `tests/**`、`error/code.rs`、`claims.rs`、`regex_cache.rs` | **不处置**：`module_inception` 需重命名模块/文件（收益低）；`should_implement_trait` 改 `From`/`Deref` 会动公开 API；`redundant_clone` 为显式 deny 下已知假阳性；内层 112 为既定测试豁免模式 | — |
