# synapse-rust 全维度审查与结构化问题清单

> 审查日期：2026-10-22
> 审查基线：HEAD `d3d41618b`（工作区另有并发会话未提交改动，**未纳入审查、未触碰**）
> **复核基线：HEAD `bf15f173a`**（同日第二轮逐条复核；更正见 §0.1，被改写的条目正文带 `> **复核**：` 标记）
> 审查方式：静态审查（逐文件阅读 + 全仓 grep 计数 + 定点交叉复核）；**未运行完整测试套件**，因此除 §0.1 明确标注"已修/已实测"者外，下文所有"行为"结论均为代码事实推断，不含运行时实测
> 严重度定义：
> - **P0**：导致数据丢失/服务不可用/认证绕过，须立即修
> - **P1**：破坏规范语义、产生不可见数据、或形成系统性安全/一致性问题，本迭代必修
> - **P2**：明确的正确性/性能/可维护性缺陷，有清晰修法，排期修
> - **P3**：规范漂移、文档失真、死代码，随手修

## 0. 总览

| 维度 | P0 | P1 | P2 | P3 | 小计 |
|---|---|---|---|---|---|
| 架构设计 | 0 | 1 | 4 | 1 | 6 |
| 代码质量 | 0 | 3 | 6 | 2 | 11 |
| 功能完整性 | 0 | 1 | 2 | 2 | 5 |
| 性能表现（含数据层） | 0 | 1 | 8 | 3 | 12 |
| 安全性 | 0 | 1 | 7 | 7 | 15 |
| 可维护性 | 0 | 0 | 4 | 4 | 8 |
| 测试覆盖 | 0 | 1 | 5 | 1 | 7 |
| 文档完整性 | 0 | 0 | 3 | 6 | 9 |
| **八维度小计** | **0** | **8** | **39** | **26** | **73** |
| 九、专项：冗余与过度开发 | 0 | 0 | 10 | 17 | 27 |
| **合计** | **0** | **8** | **49** | **43** | **100** |

**结论摘要**：无 P0；8 条 P1 中有 3 条会直接造成"数据写进库但对客户端不可见/不可达"（ARCH-01、FUNC-03、PERF-02），3 条是认证与缓存一致性（SEC-01、CODE-02、CODE-03），1 条是测试替身泄漏到生产 API（CODE-05），1 条是联邦信任根缺测试（TEST-01）。

## 0.1 第二轮复核：改了哪些结论

复核方式：对每条 P1 与全部带数字/行号的论断做定点实测（读代码、`grep -c`、`wc -l`、对照迁移与配置），不复用初审结论。
**结论：主体成立，但有 4 条实质错误、2 条口径错误必须更正**——其中 3 条是"把已实现的能力当成缺失"，正是这类审查最危险的错误方向（会诱导重复开发或误判合规风险）。

| 条目 | 初审结论 | 复核判定 | 处置 |
|---|---|---|---|
| **PERF-01** | P1：每次 `/sync` **全量**读出 to_device 并反序列化，内存随积压线性上升 | **❌ 主结论不成立**。sync 实际走 `get_messages_since(user, device, since_stream_id, limit)`：SQL 是 `WHERE stream_id > $3 … LIMIT $4`（增量 + 有界）。被点名的 `get_messages`/`get_and_delete_messages` **零调用方** | 降 **P2**；改述为"该表**从未被清理** + 两个函数是死 API" |
| **FUNC-01** | P1：`erase=true` 直接 501 ⇒ **账号注销能力缺失** | **⚠️ 一半错**。`erase=false`（默认）注销完全可用；只有 GDPR 擦除未实现，且代码在**任何副作用之前**返回 501（fail-closed，有注释说明"宁可拒绝也不假装擦除"） | 降 **P2**；改述为"擦除语义未实现（显式拒绝）" |
| **FUNC-02** | P1：3 个 voice 端点"恒 404 占位缺陷，宣告存在却永不工作" | **❌ 与本文 RD-19 自相矛盾**。同一组端点在 RD-19 被正确判定为 "MSC3245 by-design、不要删"；本轮实测确认 handler 返回 `not_implemented`（→ 404 `M_UNRECOGNIZED`）并带 MSC3245 说明 | 降 **P3**，与 RD-19 合并，不再单列为缺陷 |
| **PERF-10** | P2：PostgreSQL **无全局 `statement_timeout`** | **⚠️ 口径错**。应用已为每个连接 `SET statement_timeout='30s'`（`database_initializer/mod.rs:506`）并配置 `idle_in_transaction_timeout_secs: 60`（`homeserver.yaml:126`）；只有 **PG 实例级**没设，直连 psql 的场景不受保护。且原文路径 `docker/postgres/postgres.conf` **不存在**（实际 `docker/config/postgres.conf`） | 降 **P3**；改述为"仅实例级缺失" |
| **PERF-06** | P2：10 个 prune 全在 `synapse-services/src/tasks/pruning.rs` | 条目数 ✅（实测 10 个 `prune_*`），但**该文件不存在**；实际位置 `synapse-storage/src/pruning.rs` | 保留 P2，**改路径** |
| **CODE-11** | P3：`#[allow]` "无收敛策略"、"无法判断是例外还是掩盖" | **⚠️ 一半错**。仓里已有 `scripts/ci/check_clippy_allow_ratchet.py` + `clippy_allow_baseline` + `clippy_allow_allowlist`（棘轮 + 显式白名单）。成立的部分只有：**无 `// reason:` 强制** | 保留 P3，改述 |
| **SEC-10** | P3：URL 预览把用户 URL 写入日志 | **✅ 成立**（引用的是本轮新增文件的第 104 行） | **本轮已修**：改为只记 `scheme://host + ≤64 字符路径`，query/fragment 全丢；新增 1 条用例锁死 |
| 其余带数字论断 | ARCH-05（7 文件 >2000 行）、ARCH-06、CODE-02（5 处）、CODE-03（6 处）、CODE-09、RD-01、RD-12、RD-14（4189 行）、RD-20~22、PERF-03/04/05、TEST-03（7 条）、TEST-04（8 脚本）、SEC-04/07/08 | **✅ 逐条实测成立**（行数、计数、SQL 形态全部对上） | 保留 |

复核同时新增 1 条初审遗漏的冗余项（**RD-27**：to_device 的 4 个存储方法零调用），并订正 3 处自引用矛盾（附录 B、RD-14 标题、CODE-11 计数），见各条目下的 `> **复核**：`。

---

## 一、架构设计

### ARCH-01（P1）Thread 双实现：非规范 API 写入影子表，数据对 `/sync`、`/messages`、联邦不可见
> **复核（2026-10-22 第二轮）**：核心结论 ✅ 成立，且比原文更硬：除 `thread/storage.rs` 外
> **全仓没有任何代码写 `thread_roots`/`thread_replies`**（实测 `grep -rn thread_roots\|thread_replies
> synapse-{services,storage,web}/src` 去掉 storage/test/mock 后为 0），因此规范 `m.thread` 路径
> 也不会回填这两张表——双向不一致成立。两处需**精确化**：
> ① 端点实际是 **`/_matrix/vendor/v1/threads…`**（`handlers/thread.rs:320-329`）——
> 用 vendor 前缀本身是**正确**的（私有扩展就该放这里），缺陷不是命名空间而是**并行数据模型**；
> ② "不参与权限校验"不准确：写路径有 `ensure_thread_room_access` + `reject_unsupported_thread_content`，
> 缺的是"影子行不参与事件级 auth / state resolution"。
- **位置**
  - `synapse-services/src/thread_service.rs:273-313`（`create_thread`）、`:315-368`（`add_reply`）
  - `synapse-storage/src/thread/storage.rs:190-252`（`create_thread_reply`：仅 `INSERT thread_replies` + `UPDATE thread_roots`）
  - 路由：`synapse-web/src/routes/handlers/thread.rs:320-329`（`/_matrix/vendor/v1/threads…`）
  - 规范路径：`synapse-web/src/routes/relations.rs:329-340`（`m.thread` 走普通事件落地）
- **类型**：架构分层错误 / 数据一致性
- **影响范围**：通过 vendor thread API 创建的 thread 与回复只存在于 `thread_roots` / `thread_replies` /
  `thread_relations` 三张表；不生成 PDU、不进 `events`、不进 `/sync`、不进 `/messages`、不参与联邦与
  state resolution。反向亦不成立：客户端用规范 `m.thread` 关系发的事件**不会**出现在该 API 的列表里。
  ⇒ 同一个房间的两类客户端会看到**互不可见**的两套 thread。
- **证据**：`create_thread_reply` 事务体内无 `events` 写入；`create_thread_root` 的调用方只有
  `thread_service` 与其测试；三张表无第二个写者。
- **修法**：删除自定义 thread 写路径，改为在 `relations.rs` 的 `m.thread` 分支上派生 thread 元数据
  （或直接以 `events` + `relates_to` 查询替代 `thread_roots`/`thread_replies`）。
  若产品确需该 vendor API，最小修法是让它**写真实事件**（`m.thread` 关系）并把影子表降级为索引。

### ARCH-02（P2）`sync` 与 `sliding_sync` 无共享语义基座（非"重复实现"）
- **位置**：`synapse-services/src/sync_service/mod.rs:39`、`synapse-services/src/sliding_sync_service/mod.rs:62`
- **类型**：抽象层缺失 / 同语义两处实现
- **证据**：全仓成对交叉比对**未发现复制粘贴的函数体**——两条路径各有独立实现，差异来自 MSC3575 与 v3 sync 的协议差异，不构成"重复代码"。
- **影响范围**：懒加载成员、时间线裁剪、`limited` 语义、账户数据等**同一套语义**需在两处各写一遍；已出现过的 sync 回归（如 limited timeline 重放成员变更）只影响其中一条路径，说明语义漂移已经发生。
- **修法**：抽出共享的 timeline/state 组装层，两套服务只做协议差异适配；先用两组等价断言（同一房间状态下两路径的可见事件集合一致）锁住语义，再做抽取。

### ARCH-03（P2）"未实现"存在两套错误语义
- **位置**：`synapse-common/src/error/code.rs:449-455`（`not_supported` → 501 `M_UNKNOWN`）vs `not_implemented()` → 404 `M_UNRECOGNIZED`
- **类型**：协议一致性
- **影响范围**：同一客户端对不同未实现能力会收到 404 或 501 两种响应，无法据此做统一降级；`M_UNKNOWN` 与 `M_UNRECOGNIZED` 的语义被混用。
- **修法**：统一为单函数，按"端点不存在（404）"与"端点存在但功能未实现（501 `M_UNRECOGNIZED`）"二选一，全仓替换。

### ARCH-04（P2）`events` 未分区，`room_memberships` 无归档，保留策略按单条删除
- **位置**：`migrations/00000000_unified_schema_v12.sql:328`、`:4704`（注释保留未实现）；`synapse-services/src/admin/retention.rs:264-280`
- **类型**：容量与生命周期设计缺失
- **影响范围**：消息表随运行时间线性膨胀，索引与 autovacuum 成本持续上升；`DELETE` 单条执行会产生长事务与大量死元组，大房间上可能拖垮 WAL 与复制延迟。
- **修法**：按 `origin_server_ts` 做声明式分区（或至少按月归档表）；保留策略改分批 `DELETE ... WHERE ctid IN (SELECT ctid ... LIMIT n)` 循环。

### ARCH-05（P2）7 个超大文件（>2000 行），职责混合
- **位置**：`synapse-storage/src/room/mod.rs`（2531）、`synapse-storage/src/membership/mod.rs`（2502）、`synapse-storage/src/refresh_token/mod.rs`（2466）、`synapse-common/src/error.rs`（2442）、`synapse-common/src/config/mod.rs`（2332）、`synapse-storage/src/device/mod.rs`（2301）、`synapse-federation/src/client.rs`（2086）
- **类型**：可维护性
- **影响范围**：单文件承载多个职责，改动冲突率高、审查成本高、编译增量大。
- **修法**：按已有域边界拆分（`refresh_token` 可拆 storage/repository/rotation；`error.rs` 可拆 error + code + mapping）。

### ARCH-06（P3）Schema 规范漂移：44 处 `updated_ts BIGINT NOT NULL`、2 处可空 `created_ts`、3 处列名 `timestamp`
- **位置**：`migrations/00000000_unified_schema_v12.sql:1672`、`:1872`（可空 `created_ts`）；`:1094`、`:2684`、`:3235`（列名 `timestamp`）
- **类型**：规范一致性（且规则 §2.2 自身把 `updated_ts` 定义为 NULLABLE，与 44 处 NOT NULL 冲突）
- **影响范围**：ORM 映射与 `Option<i64>` 判断容易写错；规则文档与实际 schema 互相矛盾。
- **修法**：先修规则（二选一定稿），再补一次性对齐迁移。

---

## 二、代码质量

### CODE-01（P2）生产路径 `unreachable!` 会 panic，且无编译期或 CI 门禁
- **位置**：`synapse-common/src/crypto.rs:260`
- **类型**：错误处理 / 可用性
- **影响范围**：`hash_token` 在 release 构建且未设置 `TOKEN_HASH_SECRET` 时直接 panic。主服务启动有 `validate_token_hash_secret`（`src/server/mod.rs:182`）兜底，但库函数本身对任意其他进程（worker/工具/新增二进制）都是硬 panic；`src/bin` 下 8 个可执行入口均未校验。
- **修法**：改为返回 `Result`，或把 secret 在进程启动时注入为不可变 `OnceCell` 并让 `hash_token` 只做取值。

### CODE-02（P1）联邦成员变更路径 5 处静默吞掉缓存失效
- **位置**：`synapse-services/src/room/membership/federation.rs:480,531,748,1001,1110`（`let _ = self.cache.delete(&format!("room_state:{room_id}")).await;`）
- **类型**：错误吞噬 / 一致性
- **影响范围**：远端 join/leave/ban 后若删除失败，本节点继续用旧 `room_state` 判定成员资格与事件鉴权，最长到 TTL 到期；期间可能对已退房用户放行、对已加入用户拒绝。
- **修法**：失效失败记 `warn!` 并降级为短暂本地标记（或重试队列），不得 `let _ =`；与 CODE-03、PERF-08 统一为一处 `invalidate_room_state` helper。

### CODE-03（P1）设备删除路径 6 处吞掉懒加载成员清理失败
- **位置**：`synapse-storage/src/device/mod.rs:494,634,673,730,770,794`
- **类型**：错误吞噬
- **影响范围**：设备/用户删除后残留 `lazy_loaded_members` 行，`/sync` 会把这些成员当作"客户端已知"而抑制状态事件，表现为客户端看不到本应出现的成员变更。D-37 只修了 `:566` 一处，同类问题仍在。
- **修法**：与 `:566` 同样的处理方式统一 6 处；并给该族函数加"返回值必须被处理"的 lint（`must_use`）。

### CODE-04（P2）邀请认领时静默丢弃房间配置
- **位置**：`synapse-services/src/one_time_invite_service.rs:139`
- **类型**：错误吞噬
- **影响范围**：`serde_json::from_value(claimed.room_config).unwrap_or_default()` 在配置结构演进/字段不匹配时退化为默认配置，房间以错误参数创建且无任何日志。
- **修法**：`map_err` 成显式错误（或 `warn!` + 保留原始 JSON 落库以便重放）。

### CODE-05（P1）`FakeUserStore` 的 `unimplemented!` 未做测试门禁即对外导出
- **位置**：`synapse-storage/src/user_store_fake.rs:64`；导出点 `synapse-storage/src/lib.rs:160-161,230`
- **类型**：测试替身泄漏到生产 API
- **影响范围**：任何下游误用 `FakeUserStore` 都会在运行时 panic；且该类型是公开 API 的一部分，破坏了"生产 crate 不暴露测试替身"的边界。
- **修法**：`#[cfg(any(test, feature = "test-utils"))]` 门禁 + 未实现的方法返回 `Err` 而非 `unimplemented!`。

### CODE-06（P2）9 个函数超过 150 行，最长 461 行
- **位置**：`synapse-services/src/room/membership/federation.rs:137`（461 行）、`:782`（240 行）；`synapse-services/src/media_service.rs:213`（224 行）；`synapse-federation/src/keys.rs:397`（197 行）；`synapse-services/src/state_resolution.rs:552`（179 行）
- **类型**：复杂度
- **影响范围**：单函数多分支嵌套，难以单测；上述吞错（CODE-02）正集中在这些函数内。
- **修法**：按"解析→校验→落库→失效"四段拆分。

### CODE-07（P2）客户端请求格式错误被映射为 500
- **位置**：`synapse-web/src/routes/dm.rs:78`
- **类型**：HTTP 语义错误
- **影响范围**：客户端拿到 `M_UNKNOWN`/500 会当作服务端故障重试，放大无效请求；监控误报。
- **修法**：改 `ApiError::bad_request` / `invalid_param`。

### CODE-08（P2）已注册端点恒返回 403，且以无理由 `#[allow(dead_code)]` 掩盖
- **位置**：`synapse-web/src/routes/directory_reporting.rs:299-311`
- **类型**：死端点 / 静态检查抑制
- **影响范围**：路由存在但永远拒绝，客户端无法区分"无权限"与"未实现"；`allow(dead_code)` 隐藏了未使用代码。
- **修法**：未实现则不注册（并从 ROUTE_CONTRACT 移除），或实现后删除 `allow`。

### CODE-09（P3）常量注释与取值不符
- **位置**：`synapse-common/src/constants.rs:109-110`（注释 "1 year"，实际 `5 * 60`）
- **类型**：注释错误
- **影响范围**：以注释为准的实现方会误判 TTL。
- **修法**：改正注释或取值。

### CODE-10（P2）全仓 1411 处 `let _ =` / `if let Ok(` / `unwrap_or_default()`，无收敛策略
- **位置**：300 个文件（分布统计见本次 grep 结果）
- **类型**：错误吞噬面
- **影响范围**：CODE-02/03/04 都是这一模式的实例；没有 lint 或棘轮，新增静默失败无法被拦截。
- **修法**：对"必须处理"的返回值加 `#[must_use]`；引入 clippy `let_underscore_must_use` 并设基线棘轮。

### CODE-11（P3）268 处 `#[allow(...)]` / 117 文件，**无 `// reason:` 强制**
> **复核（2026-10-22 第二轮）**：两处更正。① 文件数是 **117**（原文 128，偏多 11）；
> ② 原文称"无收敛策略、无法判断抑制是已知例外还是掩盖问题"**不准确**——仓里已有
> `scripts/ci/check_clippy_allow_ratchet.py` + `clippy_allow_baseline` + `clippy_allow_allowlist`
> 三道治理：棘轮防增量、白名单记录已知例外。**真正缺的只有"每个 allow 必须带理由"这一步**。
- **位置**：全仓（实测 `grep -rnE '#\[allow\(' --include='*.rs'` ⇒ 268 处 / 117 文件）
- **类型**：抑制可读性
- **影响范围**：棘轮挡住了"新增无谓抑制"，但白名单里的条目**看不出为什么**，读代码时无法区分
  "已知例外（如 `clippy::unused_async` 用于必须保持签名的 stub）"与"当年为了过门禁随手加的"。
- **修法**：在 `check_clippy_allow_ratchet.py` 上再加一条：新条目必须在同行或上一行带
  `// reason: …`；老条目按白名单惰性迁移。**不要**重做棘轮本身——它已经在工作。

---

## 三、功能完整性

### FUNC-01（P2）`erase=true` 显式 501：GDPR 擦除未实现（注销本身可用）
> **复核（2026-10-22 第二轮）**：原判 P1 的框架（"账号注销能力缺失"）**不成立**。
> `erase=false`（默认）走完整注销；只有 `erase: true` 被拒，且代码在**任何副作用之前**拒绝
> （`account_compat.rs:599-606` 注释："accepting `erase: true` and deactivating without erasing
> would answer success to an instruction we never carried out, which is worse than an honest 501"）。
> 这是**有意的 fail-closed**，不是能力缺失；风险面从"注销不可用"缩小为"擦除需另行实现"。
- **位置**：`synapse-web/src/routes/account_compat.rs:602-606`
- **类型**：规范可选能力未实现（显式拒绝）
- **影响范围**：客户端若固定带 `{"erase": true}` 会得到 501 并**不会**被注销——用户侧表现为
  "注销失败"，需文档/客户端提示；不存在"静默只注销不擦除"的数据残留风险。
- **修法**：二选一并写进能力台账——① 实现擦除（`deactivate + 数据清理` 已有
  `AccountDeactivationService`，缺的是"用户内容擦除"这一层）；② 在 `API_COVERAGE_REPORT.md`
  登记为"支持注销、不支持 erase"，并让 501 的 message 指向该文档。

### FUNC-02（P3）voice `convert`/`optimize`/`transcription` 恒定不可用 —— **与 RD-19 是同一件事，按 by-design 处理**
> **复核（2026-10-22 第二轮）**：本条原为 P1，称三端点"宣告存在却永不工作"属缺陷；
> 但同一份文档的 RD-19 把它们正确判为 "MSC3245 规定语音处理在客户端、intentionally return
> `M_UNRECOGNIZED`、不删"。两条结论互相矛盾，**本轮按 RD-19 为准**。
> 实测：`synapse-web/src/routes/voice.rs:368-399` 三个 handler 均返回
> `ApiError::not_implemented(...)`（内部 `ApiErrorKind::NotFound` + `M_UNRECOGNIZED` ⇒ **404**，
> 不是 501），message 明确写"handled client-side per MSC3245"。
- **位置**：`synapse-web/src/routes/voice.rs:368,379,390`
- **类型**：可选能力未实现（by-design）
- **影响范围**：客户端得到 404 `M_UNRECOGNIZED`，按 MSC 属正常降级信号；无数据或安全影响。
- **修法**：**不删**（属 MSC 合规声明）；只在 `ROUTE_CONTRACT.md` 的"有意不实现"清单登记，
  并让 `get_voice_config` 的 `server_side_processing: false` 作为同一决策的机器可读声明（已存在）。

### FUNC-03（P1）分片上传完成时绕过配额记账
- **位置**：`synapse-services/src/media/mod.rs:377-412`（调用 `media_service.upload_media_with_id`，未经 `record_upload_usage`）
- **类型**：配额逃逸
- **影响范围**：所有走分片上传的媒体不计入用户/房间配额，配额限制可被绕过；与整块上传的记账口径不一致。
- **修法**：在分片完成路径复用 `record_upload_usage`，并把"记账"下沉到 `upload_media_with_id` 内部，消除调用方分歧。

### FUNC-04（P2）`API_COVERAGE_REPORT.md` 两处能力标注与实现相反
- **位置**：`docs/synapse-rust/API_COVERAGE_REPORT.md:384`（称 `animated` 未实现，实际 `synapse-web/src/routes/media/download.rs:27,190,284,302-308` 已实现）、`:399`（称 `M_APPSERVICE_LOGIN_UNSUPPORTED` 不存在，实际 `synapse-common/src/error/code.rs:46,140,266`）
- **类型**：能力台账失真
- **影响范围**：以台账排期会重复开发；对外兼容性声明不实。
- **修法**：台账改为由代码扫描生成（与度量单一化同源），禁止手抄。

### FUNC-05（P3）联邦能力项有意不注册，但未在任何文档中标注为设计决策
- **位置**：`/_synapse/admin/v1/backups`（无路由）
- **类型**：能力台账与实现边界不清
- **影响范围**：审查者/客户端会误判为遗漏。
- **修法**：在 ROUTE_CONTRACT 增"有意不实现"清单及理由。

---

## 四、性能表现（含数据层）

### PERF-01（P2）`to_device_messages` 只增不减；其"全量读取"API 是死代码
> **复核（2026-10-22 第二轮）**：原判 P1 的主结论**不成立**——`/sync` 并不全量读取。
> `get_to_device_events` → `get_messages_since(user, device, since_stream_id, limit)`，
> SQL 为 `WHERE stream_id > $3 … LIMIT $4`（`synapse-e2ee/src/to_device/storage.rs:365-390`），
> 上限来自 `performance.sync_to_device_limit`。被原文点名的 `get_messages`（`:332`，无 LIMIT）
> 与 `get_and_delete_messages`（`:461`）**全仓零调用方**（见 RD-27）。
> 仍然成立、且更值得修的是**另一半**：该表**没有任何保留/清理任务**。
- **位置**
  - 无清理：`synapse-storage/src/pruning.rs` 有 10 个 `prune_*`，其中 `prune_old_to_device_transactions`
    清的是**去重事务表**，**没有**针对 `to_device_messages` 的条目；服务层与 `src/` 中也没有任何
    引用该表的清理代码（实测 `grep -rn to_device_messages synapse-services/src src/` 为 0）
  - 死 API：`synapse-e2ee/src/to_device/storage.rs:332`（`get_messages`）、`:461`
    （`get_and_delete_messages`）、`:504`（`delete_messages`）、`:520`（`delete_messages_up_to`）
- **类型**：生命周期缺失 + 死 API
- **影响范围**：长期离线、或永不确认的客户端会让**该设备**的队列无限增长（数据面问题真实存在，
  但不会像原文所说"每次 /sync 全量读出拖垮 worker"——读路径本身有界）。
  表长期膨胀会拖慢 `stream_id` 索引并放大备份体积。
- **修法**：① 把 `to_device_messages` 纳入 `pruning.rs`（按 `created_ts` + 是否已投递）；
  ② 删除 4 个零调用方法，或在 `get_messages_since` 投递成功后按 `stream_id` 上限清理（需先定
  "客户端未确认也算已投递"的口径）。

### PERF-02（P1）注册流程 5-6 次独立 autocommit，无外层事务
- **位置**：`synapse-services/src/auth/register.rs:82-107`（`create_user` → `update_displayname` → `get_or_create_device_id` → `generate_access_token` → `generate_refresh_token`）；已有未使用的事务版本 `synapse-storage/src/user/storage.rs:335`（`create_user_tx`）
- **类型**：原子性缺失 / 多余往返
- **影响范围**：任一后续步骤失败会留下"有用户无设备无 token"的半成品账号，用户名被占用且无法重试注册；同时 5 次网络往返放大注册延迟。
- **修法**：用 `create_user_tx` 打开事务，把设备与 token 一并写入；displayname 失败不应静默（现为 `warn!`，见 CODE-04 同类）。

### PERF-03（P2）房间名模糊搜索的索引列不匹配
- **位置**：`synapse-storage/src/room/models.rs:407-409,465-467`、`synapse-storage/src/room/mod.rs:550`（`LOWER(r.name) LIKE`）vs 索引 `idx_rooms_name_trgm`（建在原始 `name` 列）
- **类型**：索引用不上
- **影响范围**：房间目录搜索退化为全表扫描，房间量大时单次搜索可达秒级。
- **修法**：索引改 `CREATE INDEX ... ON rooms USING gin (LOWER(name) gin_trgm_ops)`，或查询去掉 `LOWER()`（统一写入时小写化）。

### PERF-04（P2）消息搜索用 `content::text LIKE`，无法命中 GIN
- **位置**：`synapse-storage/src/event/search.rs:26,82-84`
- **类型**：索引用不上
- **影响范围**：`search` 对 `events` 全表扫 + JSON 转文本，是最重的查询之一；`events.content` 的 `GIN(jsonb_path_ops)` 完全未参与。
- **修法**：改用 `content @> ...` 或建立 `tsvector` 生成列 + GIN。

### PERF-05（P2）成员查询使用前导通配 `LIKE '%:%'`
- **位置**：`synapse-storage/src/membership/mod.rs:1128`
- **类型**：索引用不上
- **影响范围**：无法走 B-tree，成员多的房间上每次调用都是全表/全索引扫。
- **修法**：用 `POSITION(':' IN user_id) > 0` 或增加 `is_local BOOLEAN` 派生列并建索引。

### PERF-06（P2）10 个 prune 全部无分批；保留策略单条删除
> **复核（2026-10-22 第二轮）**：条目数 ✅（实测 10 个 `prune_*`），但**原文路径不存在**：
> `synapse-services/src/tasks/pruning.rs` 无此文件（`synapse-services/src/tasks/` 目录本身不存在），
> 实际位置是 `synapse-storage/src/pruning.rs:99-245`。
- **位置**：`synapse-storage/src/pruning.rs`（10 个 prune 入口）；`synapse-services/src/admin/retention.rs:264-280`
- **类型**：长事务 / 锁放大
- **影响范围**：首次清理或积压后单条 `DELETE` 形成长时间锁与巨量死元组，期间写入抖动、复制延迟上升。
- **修法**：统一 `DELETE ... WHERE id IN (SELECT id ... LIMIT :batch)` 循环 + `VACUUM` 触发。

### PERF-07（P2）3 处列表查询无 `LIMIT`
- **位置**：`synapse-services/src/media/chunked_upload.rs:294,302`、`synapse-services/src/invite_blocklist.rs:150`
- **类型**：无界查询
- **影响范围**：单用户/单房间的积压数据直接决定响应体大小与内存占用，可被构造为内存放大攻击面。
- **修法**：强制 `LIMIT` 上限并从配置读取。

### PERF-08（P2）5 处 OFFSET 深分页
- **位置**：`synapse-storage/src/user/storage.rs:1543-1558`、`synapse-storage/src/invite_blocklist.rs:386,419`、`synapse-storage/src/sliding_sync/repository.rs:301`、`synapse-web/src/routes/handlers/room/events.rs:568-593`（先取 `from+limit` 再内存切片）
- **类型**：深分页退化
- **影响范围**：页码越深越慢，且内存切片版会在服务端把已跳过的行也读出来。
- **修法**：改 keyset（`WHERE (ts, id) < (:ts, :id)`）分页；`events.rs` 直接用 SQL 的 `OFFSET/LIMIT` 或游标。

### PERF-09（P2）邀请黑名单 upsert 缺 `FOR UPDATE`，存在并发竞态
- **位置**：`synapse-storage/src/invite_blocklist.rs:53-76,112-135`
- **类型**：并发正确性
- **影响范围**：并发重复拉黑/解除会读到旧行并覆盖对方写入，导致计数或状态丢失。
- **修法**：`SELECT ... FOR UPDATE` 或改为单条 `INSERT ... ON CONFLICT DO UPDATE`。

### PERF-10（P3）PostgreSQL **实例级**未设 `statement_timeout`（应用侧已设）
> **复核（2026-10-22 第二轮）**：原判 P2 称"无全局 `statement_timeout`，连接池可被鲸吞"，
> 与实现不符——应用**逐连接**设置了它：`database_initializer/mod.rs:506`
> `SET statement_timeout = '30s'`，配置项 `database.statement_timeout_secs: 30`（`homeserver.yaml:124`）
> 与 `idle_in_transaction_timeout_secs: 60`（`:126`）都在用。原文给的两个修法**一半已经实现**。
> 原文路径 `docker/postgres/postgres.conf` 也不存在（实际 `docker/config/postgres.conf`，
> 该文件确未设这两项）。
- **位置**：`docker/config/postgres.conf`（未设）；应用侧实现见 `synapse-services/src/database_initializer/mod.rs:500-530`
- **类型**：资源保护（仅剩"绕过应用直连"的缺口）
- **影响范围**：通过连接池的查询已被 30s 截断；**直连 psql / 迁移脚本 / 外部工具**没有保护，
  一条失控查询仍可长期占用 PG 资源。`idle in transaction` 同理（应用侧 60s，实例级无限）。
- **修法**：在 `postgres.conf` 补 `statement_timeout = 30s`、`idle_in_transaction_session_timeout = 60s`
  作为兜底，并对管理类长任务用 `SET LOCAL` 放宽——注意与应用侧口径保持一致，否则会出现
  "应用说 30s、实例说 120s"的两种真相。

### PERF-11（P3）N+1 查询仅 3 处（风险可控）
- **位置**：`synapse-services/src/room/lifecycle/create.rs:611-643`、`synapse-services/src/sliding_sync_service/filters.rs:178-186`、`synapse-services/src/user_service.rs:304,366`
- **类型**：N+1
- **影响范围**：前者在创建大房间时按成员逐个查询，房间成员数直接放大往返次数。
- **修法**：改批量 `WHERE ... = ANY($1)`。

### PERF-12（P3）索引台账计数与 SQL 实际不一致
- **位置**：`migrations/INDEXES.md`（声明 342）vs `migrations/00000000_unified_schema_v12.sql`（实际 347）
- **类型**：台账失真
- **影响范围**：无法用台账做索引变更审计。
- **修法**：台账由 SQL 解析脚本生成。

---

## 五、安全性

### SEC-01（P1）RBAC 总开关关闭时，`super_admin` 专属黑名单一并失效（fail-open）
- **位置**：`synapse-web/src/utils/admin_auth.rs:66-67`（`let rbac_allowed = !rbac_enabled || allowed;`）；黑名单定义 `synapse-web/src/utils/admin_auth/privileged.rs:15-60`
- **类型**：授权绕过（配置相关）
- **影响范围**：`security.admin_rbac_enabled=false` 时，任何 `is_admin=true` 的账号（含 `user_admin`、`auditor` 等）都能访问只应属于 `super_admin` 的高危端点。开关的本意是"关闭角色细分"，而不是"关闭超管专属限制"，当前实现把两者耦合了。
- **证据**：`is_role_allowed` 内部先判 `super_admin` 再判黑名单；短路后 `allowed` 完全不被参考。
- **修法**：把 `is_super_admin_only_endpoint(method, path)` 提到开关外，做成独立的无条件拒绝；开关只影响 `allowlist::role_allows`。

### SEC-02（P2）默认管理员口令 `Admin@123` 在脚本与文档中扩散
- **位置**：`docker/deploy/run_permission_matrix.sh:33`（硬编码）、`api-integration_test.sh:40`、`scripts/api_test/token_manager.py:195`、`scripts/api_test/config.yaml:37`、`scripts/api_test/run-load-test.sh:10`
- **类型**：凭据卫生
- **影响范围**：`scripts/api_test/register_admin.py:19-25` 已强制环境变量，不会自动进生产；但脚本中的硬编码值会被复制到部署环境。
- **修法**：脚本统一从环境变量读取，默认值仅限 `127.0.0.1` 且显式 `TEST_ONLY`。

### SEC-03（P2）MFA 配置只校验"非空"，占位符可当 TOTP 密钥
- **位置**：`synapse-common/src/config/validation.rs:17-21`；`synapse-web/src/utils/admin_auth.rs:269-276`（`decode_base32_secret(...).or_else(|| Some(trimmed.as_bytes().to_vec()))`）；`.env.example:60` 的 `CHANGE_ME`
- **类型**：配置校验缺失
- **影响范围**：误用示例值时 MFA 形同虚设（任意人可用固定密钥推码），且不会报错。
- **修法**：拒绝已知占位符与过短密钥，非法 base32 直接启动失败而非退化为字节串。

### SEC-04（P2）未设置 `PROMETHEUS_AUTH_TOKEN` 时 `/metrics` 完全匿名
- **位置**：`src/server/mod.rs:1319-1322`
- **类型**：信息暴露
- **影响范围**：指标含房间数、用户数、请求分布、错误率，可被外部枚举业务规模与攻击面。
- **修法**：无 token 时默认仅绑定 loopback，或启动告警并要求显式 `metrics.public = true`。

### SEC-05（P2）MFA 敏感操作只覆盖写方法与 3 个路径前缀，读接口可无 MFA 取明文凭据
- **位置**：`synapse-web/src/utils/admin_auth.rs:209-217`
- **类型**：MFA 覆盖缺口
- **影响范围**：`GET .../registration_tokens`、`GET .../appservices` 返回明文 token/as_token，仅凭 access token 即可读取；一旦管理 token 泄漏，MFA 不能阻断凭据二次窃取。
- **修法**：把"返回凭据类"的 GET 纳入敏感集合（按响应语义而非方法判断）。

### SEC-06（P2）审计日志来源 IP 可经 `X-Forwarded-For` 伪造
- **位置**：`synapse-web/src/middleware/auth.rs:251-256`（以 `peer_addr=None, trusted_proxies=&[]` 调用）；`synapse-web/src/utils/ip.rs:66-67`（无条件解析 XFF）
- **类型**：审计污染
- **影响范围**：攻击者可伪造审计记录中的来源 IP，事后溯源失效；`peer_addr=None` 时本应"不可知"，却被 XFF 填充。
- **修法**：仅在 `trusted_proxies` 命中时采信 XFF；否则只用 `peer_addr`，都无则记 `unknown`。

### SEC-07（P2）联邦重放缓存窗口 300s 与签名容差 24h 不匹配
- **位置**：`synapse-web/src/routes/state.rs:88-92`（`window_secs: 300`、进程内、`10_000` 容量）vs `synapse-common/src/config/federation.rs:285-287`（24h）
- **类型**：重放防护不足
- **影响范围**：300s 后缓存项被淘汰，签名仍在 24h 有效期内的事件可重放；进程内缓存也无法跨实例共享，多副本部署下窗口进一步缩小。
- **修法**：窗口对齐签名容差；缓存改为 Redis（已有缓存层），并设容量与淘汰策略。

### SEC-08（P2）监控栈默认口令 `admin123`
- **位置**：`docker/docker-compose.monitoring.yml:113`（`${GRAFANA_ADMIN_PASSWORD:-admin123}`）
- **类型**：默认凭据
- **影响范围**：未设环境变量时 Grafana 以弱口令暴露；若端口被映射到公网即失陷。
- **修法**：去掉默认值，缺失即启动失败。

### SEC-09（P3）CAS 回调存在开放重定向
- **位置**：`synapse-web/src/routes/cas.rs:281`
- **类型**：开放重定向
- **影响范围**：`service` 参数未做同源校验，可用于钓鱼跳转。
- **修法**：校验 `service` 必须落在允许的 client origin 列表内。

### SEC-10（P3，**本轮已修**）URL 预览把用户提供的 URL 写入日志
> **复核（2026-10-22 第二轮）**：成立，且引用的正是同日新增文件的第 104 行。
> 已修：新增 `loggable_url()`（`synapse-services/src/url_preview.rs`），只记
> `scheme://host` + ≤64 字符路径，**query 与 fragment 全部丢弃**
> （预览 URL 由调用方控制，query 里常带会话 token 与私有文档 id）；
> 截断按 char boundary 处理，避免中文路径在半字节处 panic。
> 新用例 `logged_url_drops_the_query_and_caps_the_path` 锁死；`url_preview` 16/16 通过。
- **位置**：`synapse-services/src/url_preview.rs:104`（原 `tracing::info!(url = %url, …)`）
- **类型**：隐私泄漏
- **影响范围**：URL 可能含会话 token 或私密参数，落入集中日志。
- **修法**：**已完成**。后续同类日志（联邦抓取、媒体代理）应按同一规则收敛，建议抽成公共 helper。

### SEC-11（P3）静态密钥封装存在全零密钥死代码
- **位置**：`synapse-common/src/key_at_rest.rs:98`
- **类型**：死代码 / 误用风险
- **影响范围**：全零密钥分支若被启用等于明文存储。
- **修法**：删除该分支，或改为编译期不可达。

### SEC-12（P3）legacy r0/v1/v3 媒体下载端点为匿名
- **位置**：`synapse-web/src/routes/media/download.rs:315-334`
- **类型**：访问控制不一致
- **影响范围**：新版本端点要求鉴权，旧版本端点不要求，形成降级绕过（若 media ID 可枚举/泄漏）。
- **修法**：统一鉴权要求，或在配置中默认关闭 legacy 前缀。

### SEC-13（P3）联邦鉴权 DEBUG 日志打印签名原文
- **位置**：`synapse-federation/src/.../federation_auth.rs:407`
- **类型**：敏感信息入日志
- **影响范围**：开启 DEBUG 的环境把完整签名与待签串写入日志，便于离线分析攻击。
- **修法**：只打印前 8 字符 + 长度。

### SEC-14（P3）联邦请求缺 `ts` 仅告警不拒绝
- **位置**：`.../federation_auth.rs:219-224`
- **类型**：校验宽松
- **影响范围**：无时间戳的请求可绕过时效判断，配合 SEC-07 形成重放面。
- **修法**：缺 `ts` 直接 401。

### SEC-15（P3）鉴权覆盖率约 94.4%，client/media 面存在未覆盖路由
- **位置**：client 约 301/321、media 约 28/37（federation 36/48 中 12 条为规范公开端点，属预期）
- **类型**：覆盖面缺口
- **影响范围**：未覆盖路由需逐条确认为"规范公开"或"漏挂中间件"。
- **修法**：补一份路由-鉴权对照表并加 CI 断言（新增路由必须显式声明 `public` 或 `authenticated`）。

---

## 六、可维护性

### MAINT-01（P2）测试替身、工具二进制与生产 crate 未做特性隔离
- **位置**：`synapse-storage/src/lib.rs:160-161,230`（见 CODE-05）
- **类型**：边界泄漏
- **影响范围**：生产依赖会编译测试用代码，增大产物并引入 panic 面。
- **修法**：`test-utils` feature 门禁，CI 增加 `--no-default-features` 构建。

### MAINT-02（P2）错误吞噬模式无 lint 与棘轮（见 CODE-10）
- **类型**：治理缺失
- **影响范围**：同类缺陷会持续新增。

### MAINT-03（P2）两套同步实现导致规则双写（见 ARCH-02）
- **类型**：重复实现

### MAINT-04（P2）能力台账手工维护，已出现多处失真（见 FUNC-04、DOC-01~03）
- **类型**：流程缺陷
- **影响范围**：每次实现变更都可能让台账继续偏离。

### MAINT-05（P3）5 处注释引用不存在的脚本路径
- **位置**：`src/lib.rs:10`、`synapse-common/src/lib.rs:15`、`synapse-e2ee/src/lib.rs:5`、`synapse-federation/src/lib.rs:5`、`synapse-services/src/lib.rs:9`（均写 `scripts/quality/check_missing_docs_ratchet.sh`，实际为 `scripts/check_missing_docs_ratchet.sh`）
- **类型**：失效引用
- **影响范围**：维护者按注释找不到门禁脚本，误以为门禁不存在。
- **修法**：改路径，并在 CI 增加"文档内路径存在性"检查。

### MAINT-06（P3）`LivekitConfig` 三个字段整块零业务读取
- **位置**：`synapse-common/src/config/voip.rs:81-96`（`api_key` / `api_secret` / `host`）
- **类型**：死配置
- **证据**：全仓仅 `Default` 构造与 re-export 触达该结构（`src/common/config/tests.rs:155,321,599`、`synapse-services/src/test_config.rs:230`、`synapse-services/src/lib.rs:240`），无任何代码读取三个字段。原 `ws_url` 字段已删除，删除理由见该文件 L83-86 注释（"从未被任何代码路径读取"）。
- **影响范围**：运维配置 `matrix_rtc.livekit.*` 完全不生效，形成"以为可配"的错觉；`docs/synapse-rust/API_COVERAGE_REPORT.md:416` 仍以已删除的 `LivekitConfig.ws_url` 作为证据（见 RD-26）。
- **修法**：删除整个 `LivekitConfig` 及对应配置段——`rtc/transports` 只返回 ICE 候选，不需要 SFU 凭据；未来真接入 SFU 时再按需引入。

### MAINT-07（P3）索引台账与迁移说明未同步（见 PERF-12、DOC-06）
- **类型**：文档滞后

### MAINT-08（P3）`#[allow(...)]` 无理由比例极高（见 CODE-11）
- **类型**：抑制滥用

---

## 七、测试覆盖

### TEST-01（P1）联邦签名路径测试覆盖显著偏薄
- **位置**：`signing::` 相关断言仅 14 处 / 8 个文件（对比 admin/rbac 64 处/27 文件、media 96 处/9 文件）
- **类型**：关键路径覆盖不足
- **影响范围**：签名/验签是联邦信任根，含规范化 JSON、ed25519 验签、密钥轮换三类高风险逻辑，回归只能靠人工。
- **修法**：补规范化边界（键序、unicode、嵌套数组）、过期/未来 `ts`、未知密钥、密钥轮换后旧签名四组用例。

### TEST-02（P2）6 个测试文件只断言状态码，不断言响应体
- **位置**：`api_admin_target_guard_tests.rs`、`api_security_headers_tests.rs`、`api_worker_replication_auth_tests.rs`、`msc4108_rendezvous_route_tests.rs`、`api_route_ledger_tests.rs`、`api_federation_key_fetch_limits_tests.rs`
- **类型**：断言强度不足
- **影响范围**：返回体结构/错误码变更无法被发现（本轮已因此漏过一处 401 挑战体结构变更）。
- **修法**：至少断言 `errcode` 与关键字段；结构稳定的改用 insta 快照。

### TEST-03（P2）27 条 e2e 中 7 条 `#[ignore]`，CI 实际不执行
- **位置**：`tests/e2e/user_flow_tests.rs:282,322,358,404,449,487,511`
- **类型**：测试名义存在但未运行
- **影响范围**：完整用户流程（注册→建房→发消息→同步）无自动化保护。
- **修法**：CI 加一条起本地 homeserver 的 job 专门跑这 7 条。

### TEST-04（P2）8 个 CI 脚本从未被任何 workflow 调用
- **位置**：`check_axum_path_syntax.py`（仅出现在 `paths:` 触发器）、`check_feature_matrix.py`、`check_plan_doc_sections.py`、`check_plan_numbers_fresh.sh`、`check_root_canonical_ledger.py`、`check_ts_order_tiebreak.py`、`converge_public_schema.sh`、`run_complement_tests.sh`
- **类型**：死门禁
- **影响范围**：看起来有校验，实际永不失败，形成虚假安全感。
- **修法**：逐个接入或删除；接入后必须验证"故意造错能让它失败"。

### TEST-05（P2）27 处 `tokio::time::sleep` / 14 文件，构成时序 flake 面
- **位置**：Top：`tests/e2e/user_flow_tests.rs`（7 处）、`tests/integration/room_service_tests_migrated.rs`（5 处）
- **类型**：测试稳定性
- **影响范围**：本仓库已有 2 条时序用例（appservice 调度、beacon 令牌桶）无法稳定复现，属同一类问题。
- **修法**：改条件轮询（带 deadline 的 `wait_until`）或注入可控时钟。

### TEST-06（P2）5 个整文件以 feature 门控，默认测试运行时不参与
- **位置**：`e2ee_audit_service_tests.rs:15`、`space_children_service_tests.rs:25`、`burn_after_read_route_tests.rs:21`、`server_notification_service_tests.rs:32`、`api_widget_tests.rs:1`
- **类型**：覆盖不可见
- **影响范围**：默认 `cargo nextest` 下这些能力零测试；需确认 CI 是否另有 feature 矩阵（`check_feature_matrix.py` 未接入，见 TEST-04）。
- **修法**：CI 增加 feature 组合 job，或把用例改为运行时跳过并计入统计。

### TEST-07（P3）测试规模口径三处不一致
- **位置**：`docs/synapse-rust-vs-synapse-comparison.md:413`（238 个 `.rs`）vs 实测 277（unit 126 + integration 146 + e2e 2 + perf 3）；`.trae/rules/project_rules.md` §9.1（115/130/3/5）vs 实测 126/146/2/3
- **类型**：度量漂移
- **影响范围**：无法用文档判断覆盖趋势。
- **修法**：接入已有的度量脚本，文档只引用。

---

## 八、文档完整性

### DOC-01（P2）`API_COVERAGE_REPORT.md` 能力标注与实现相反（详见 FUNC-04）
### DOC-02（P2）项目规则 §9.1 测试规模数字过时（详见 TEST-07）
### DOC-03（P2）项目规则 §10.1 把 MSC3245 标为 "Room summary"，实际 MSC3245 是语音消息
- **位置**：`.trae/rules/project_rules.md` §10.1；实现见 `synapse-web/src/routes/voice.rs:374`
- **类型**：规范误标
- **影响范围**：以规则为准的排期/验收会指向错误能力；真正的 Room summary 是 MSC3266（规则另有一行，易混淆）。
- **修法**：改标签并补一句"MSC3245 = 语音消息"。

### DOC-04（P3）三份文档共 9 组行号漂移
- **位置**：`API_COVERAGE_REPORT.md`、`MSC_SEMANTICS.md`、`ROUTE_CONTRACT.md`；例：`room_versions.rs:151`→实际约 158；`code.rs:83,131,225,292`→96/162/229/288；`app_service.rs:722-723`→766-767
- **类型**：引用失效
- **影响范围**：审查者按行号定位会看到无关代码。
- **修法**：改引用符号名（函数/常量）而非行号。

### DOC-05（P3）5 处注释引用错误脚本路径（详见 MAINT-05）
### DOC-06（P3）`UNRESOLVED_ISSUES_SUMMARY.md` 已滞后于 HEAD
- **位置**：`docs/audit/UNRESOLVED_ISSUES_SUMMARY.md`（2026-10-09，称 0 P0/0 P1、26 P2 全闭）
- **类型**：权威清单失效
- **影响范围**：与本次审查结果（**8 条 P1**，复核后口径见 §0.1）直接矛盾；读者会误判项目状态。
- **修法**：以本文件为新基线重写摘要，或把旧文件标记 `HISTORICAL`。

### DOC-07（P3）`migrations/INDEXES.md` 计数 342 vs 实际 347（详见 PERF-12）
### DOC-08（P3）`migrations/README.md` 描述的两个缺陷已修，文档未同步
- **类型**：文档滞后
### DOC-09（P3）`docs/synapse-rust-vs-synapse-comparison.md` 测试文件数 238 vs 实测 277（详见 TEST-07）

---

## 九、专项审查：冗余代码、过度开发与超前实现

> 本章为八维度清单之外的专项排查，目标四类：① 未被实际使用的冗余代码/废弃模块/无用依赖/重复实现；② 超出当前业务需求的过度开发、不必要抽象、过度设计；③ 可简化但实现过繁的逻辑；④ 无实际业务价值的预留扩展点与超前实现。
> **判定口径**：每条"未被使用"结论都要求全仓 grep 分别核对**生产路径 / 测试路径 / `cfg` 门禁**三面；仅"测试引用"不算生产使用，仅"构造/re-export"不算业务读取。子代理初筛结果已逐条定点复核，剔除的误报见 §9.11。
> 编号前缀 `RD-`（Redundancy）。严重度沿用 §0 定义（本专项无 P0/P1）。

---

### 9.1 无用依赖

#### RD-01（P2）根 crate 15 个直接依赖在 `src/**` 零引用
- **位置**：`Cargo.toml:71,74,76,85,87,88,89,94,105,106,107,108,111,121,122`
- **清单**：`async-trait`、`tower`、`hyper`、`serde_norway`、`rand`、`sha2`、`hmac`、`config`、`futures`、`regex`、`url`、`urlencoding`、`validator`、`image`、`hex`
- **证据**：`src/**` 下 `\b(<crate>)::` 与 `^use <crate>` 两种检索均 0 命中；`config` 外部 crate 另检索 `config::File|config::Environment|config::Config::builder` 亦 0 命中（`config::Config` 的命中全部来自本仓 `synapse_common::config::Config`）。
- **影响**：默认构建多编译 15 棵依赖树（含 `image`/`regex`/`hyper` 等重型 crate），冷构建与 CI 时间被放大；`cargo audit` 的告警面同步扩大。
- **建议**：逐条删除后跑 `cargo build --workspace --all-targets` + `cargo test --workspace --no-run` 验证；`hyper` 若需类型别名应由 `axum::http` 提供，不保留直连。
- **不在此列（已确认在用）**：`uuid`（`src/bin/synapse_worker.rs:86`）、`lettre`（`src/bin/synapse_worker.rs:8-9`）、`tikv-jemallocator`（`src/main.rs:10`）、`ed25519-dalek`（`src/common/error.rs:17`）、`tower-http`（`src/server/router.rs:2-3`）、`base64`（`src/bin/federation_sign_request.rs:6-7`）、`redis`/`deadpool-redis`（`src/server/services.rs:1,21-22,30`）、`reqwest`（`src/worker/heartbeat.rs:125`）。

#### RD-02（P3）`synapse-common` 3 个依赖零引用、1 个仅测试用
- **位置**：`synapse-common/Cargo.toml:37`（`tower-http`）、`:38`（`http`）、`:81`（`validator`）、`:74`（`futures`）
- **证据**：`tower-http` 唯一命中是 `src/logging.rs:34` 里的日志指令字符串 `"tower_http::trace=info"`（非 crate 使用）；`validator` 唯一命中是 `src/config/mas.rs:20` 注释；`http::` 的命中全部是 `axum::http::`（`src/error.rs:5-9`、`src/error/code.rs:5`）。`futures::` 仅出现在 `src/password_hash_pool.rs:515,620`，位于 `#[cfg(test)] mod tests`（`:461`）内。
- **影响**：三个 crate 编译后完全未被链接使用；`futures` 本可降为 dev-dependency。
- **建议**：删除前三个；`futures` 移到 `[dev-dependencies]`。`serde_norway`（`src/rate_limit_config.rs:20,268,275`）为真实使用，保留。

#### RD-03（P3）`synapse-web` 3 个依赖仅测试使用，却声明在 `[dependencies]`
- **位置**：`synapse-web/Cargo.toml:70`（`futures`）、`:77`（`serde_norway`）、`:79`（`deadpool-redis`）
- **证据**：全 crate 命中各仅一处，且均落在 `#[cfg(test)]` 模块内——`routes/extractors/json.rs:81`（`use futures::stream`）、`routes/handlers/health.rs:241`（`serde_norway::to_string`，模块起点 `:216`）、`utils/login_lockout.rs:341`（`deadpool_redis::…`，模块起点 `:338`）。
- **影响**：生产构建（`--release` 非 test）无需这三个 crate，却仍被编译进依赖图。
- **建议**：整体迁到 `[dev-dependencies]`；迁移后 `synapse-web` 的 release 依赖树可直接瘦身。

#### RD-04（P3）`synapse-services` 的 `rsa` 未做 feature 门控，默认构建白编译
- **位置**：`synapse-services/Cargo.toml:69`
- **证据**：`rsa::` 的全部生产使用集中在 `src/builtin_oidc_provider.rs:29-33`（该文件由 `builtin-oidc` feature 门控）与 `src/oidc_service.rs:1040-1126` 的测试模块；`builtin-oidc` 是**非默认 feature**（`Cargo.toml:30`）。
- **影响**：默认构建编译一个非默认能力才需要的 RSA/加密栈（含 `digest 0.10` 旧代链，见项目规则 §17.5 族 A），拉长构建并加剧依赖分裂。
- **建议**：改为 `rsa = { version = "0.9", features = ["pem","sha2"], optional = true }`，并在 `builtin-oidc` feature 中加入 `"dep:rsa"`。注意 `synapse-e2ee` 的 `ed25519-dalek` 是**生产使用**（`src/signed_json.rs:4`、`src/crypto/ed25519.rs:4-5`），不可同样处理。

---

### 9.2 空转门面与死 pub API

#### RD-05（P2）`src/storage/mod.rs` 是全仓最大的空转门面（65 行纯 re-export）
- **位置**：`src/storage/mod.rs:1-65`
- **证据**：文件头 L6-8 自述"All src/ consumers now import directly from synapse_storage"，余下 40 条模块 re-export（L10-55）+ 11 条域组 re-export（L63-65）仅供测试/bench/二进制"仍走门面"。生产装配层（`src/server/*`）无任何 `crate::storage::` 引用。
- **影响**：新增/删除存储模块时要在两处维护导出；门面让"是否还有生产消费者"无法用 `cargo udeps` 直觉判断，也是本仓 `storage` 路径在文档中反复漂移的来源。
- **建议**：把测试/bench 的 `synapse_rust::storage::X` 改为 `synapse_storage::X`（纯机械替换），随后整体删除该文件与 `src/lib.rs` 中的 `pub mod storage;`。门禁：删除后 `cargo test --workspace --no-run` 必须通过。

#### RD-06（P3）`src/tasks/mod.rs` 8 个 pub 访问器零调用，且 doc 注释整段重复两遍
- **位置**：`src/tasks/mod.rs:395-443`
- **证据**：`get_last_health_status`、`get_last_performance_metrics`、`get_last_integrity_report`、`get_last_maintenance_report`、`trigger_health_check`、`trigger_performance_check`、`trigger_integrity_check`、`trigger_maintenance` 均为 `pub`，但 `src/**` 与 `tests/**` 均无调用方；每个函数的 `///` 文档行在 L395-396、L401-402 等处成对重复。
- **影响**：对外暴露了 8 个无人使用的维护入口，扩大 `src` 二进制 crate 的公开面；重复注释会被 `cargo doc` 原样输出。
- **建议**：改为 `pub(crate)` 后再删；若确认无内部调用则整体删除（调度器自身仍由 worker 主循环驱动）。

#### RD-27（P2）`to_device` 的 4 个存储方法零调用，其中一个的名字与语义**相反**
- **位置**：`synapse-e2ee/src/to_device/storage.rs:332`（`get_messages`）、`:461`
  （`get_and_delete_messages`）、`:504`（`delete_messages`）、`:520`（`delete_messages_up_to`）
- **证据**：`grep -rn "get_and_delete_messages\|\.get_messages(\|delete_messages" --include="*.rs"
  synapse-services/src synapse-web/src synapse-e2ee/src` ⇒ **0 命中**；`/sync` 实际走
  `get_messages_since`（`:365`，带 `WHERE stream_id > $3 … LIMIT $4`）。
- **影响**：不只是"多余代码"。`get_and_delete_messages` 的名字让人以为"投递即删除"，
  于是排查"`to_device_messages` 只增不减"时会**查错方向**——本文 PERF-01 的初判正是被它误导，
  据此写出"每次 `/sync` 全量拉取"的错误结论。生效的读路径**不删任何行**，与名字相反。
- **建议**：删除 4 个方法（`synapse-e2ee` 是库 crate，删 pub 方法属破坏性变更，需先确认无外部消费者）；
  若确实要"投递后清理"，在 `get_messages_since` 的调用方显式实现，并先定"客户端未确认算不算已投递"。

---

### 9.3 测试替身进入生产构建

#### RD-07（P2）`user_store_fake`（706 行）无任何 `cfg` 门禁即进入生产编译
- **位置**：`synapse-storage/src/lib.rs:160-161`（`pub mod user_store_fake;`）、`:230`（`pub use user_store_fake::FakeUserStore;`）
- **证据**：`tests/unit/...` 与 `synapse-storage/src/test_mocks/mod.rs:15` 使用；生产代码零引用，但模块无 `#[cfg(any(test, feature = "test-utils"))]`。文件规模记录在 `scripts/ci/file_size_baseline:182`（706 行）。
- **影响**：706 行测试替身被编进生产 `rlib`，增大编译时间与符号表；`FakeUserStore` 成为对外公开 API，被误用后会产生"看起来正常但不落库"的实现。
- **建议**：加 `#[cfg(any(test, feature = "test-utils"))]` 门禁，并把 `test_mocks` 一并收口；`synapse-test-utils` 已是现成的可选依赖入口。

#### RD-08（P3）`federation_test_keys` 无 `cfg` 门禁
- **位置**：`synapse-common/src/lib.rs:46-47`、`:195`（re-export）
- **证据**：唯一生产路径消费者是 `src/bin/generate_test_keypair.rs:7`（开发工具二进制），其余为测试。
- **影响**：测试密钥生成器随主库发布，属于"测试能力进生产"；虽无运行时危害，但扩大公开面。
- **建议**：与 RD-07 同批处理（`#[cfg(any(test, feature = "test-utils"))]`），`generate_test_keypair` 改为通过 `synapse-test-utils` 引入。

---

### 9.4 预留字段与容器死字段

#### RD-09（P2）`AdminServices` 自述 14 个"留给未来路由"的 storage 字段
- **位置**：`synapse-services/src/wiring/admin/models.rs:140-150`（注释）；字段定义见同文件 `:22`（`refresh_token_storage`）、`:26`（`registration_token_storage`）、`:37`（`federation_blacklist_storage`）、`:48`（`media_quota_storage`）等
- **证据**：注释原文——"The storage copies themselves are not accessed via the container after construction, but are retained for potential direct access by future route handlers. Verified unused storage fields include: `registration_token_storage`, `captcha_storage`, `audit_storage`, `feature_flag_storage`, `event_report_storage`, `background_update_storage`, `retention_storage`, `push_notification_storage`, `app_service_storage`, `app_service_event_reader`, `worker_storage`, `refresh_token_storage`, `federation_blacklist_storage`, and `media_quota_storage`."
- **影响**：14 个 `Arc<Storage>` 常驻容器，扩大 `AdminServices` 的构造面与内存占用，并制造"容器里有的东西一定是可达的"错误预期；新增路由时容易被诱导直接取用而非走既有服务边界。
- **建议**：一次性删除这 14 个字段（构造时仍可传参给需要它们的服务）；删除后 `cargo build -p synapse-services` 即是最强校验。若确需保留个别字段，必须在注释里写明"哪一个未来路由"并挂 issue。

#### RD-10（P3）`ExtensionServices` 保留字段清单（含已删除服务的注释残留）
- **位置**：`synapse-services/src/wiring/extensions.rs:9-18`
- **证据**：注释自述保留 `friend_storage`/`server_notification_storage`/`widget_storage`（作为对应 service 的 backing storage）与 `privacy_storage`（构造期消费、存储副本不再读）；并记录了"审查 #23"已删除 `friend_federation`/`user_lock_service`/`ai_connection_storage`。
- **影响**：注释本身是好的（可追溯），但 `privacy_storage` 明确"构造后不读"，属可删字段；`voice_service`（`:32-34`，`voice-extended`）等也需按 RD-14 一并评估。
- **建议**：`privacy_storage` 改为构造期局部变量；其余三个 backing storage 保留但统一加 `#[allow(dead_code)]` 注释说明，避免 clippy 噪音与误判。

---

### 9.5 零读取配置项

#### RD-11（P2）`LivekitConfig` 三字段整块无读取点
- **位置**：`synapse-common/src/config/voip.rs:81-96`
- **证据**：全仓仅 `Default` 构造与 re-export（`src/common/config/tests.rs:155,321,599`、`synapse-services/src/test_config.rs:230`、`synapse-services/src/lib.rs:240`），`api_key`/`api_secret`/`host` 零读取。
- **影响 / 建议**：同 MAINT-06（本节为专项口径下的重复登记，**修复时合并处理，只做一次删除**）。

#### RD-12（P2）`ServerConfig` 5 个字段零读取
- **位置**：`synapse-common/src/config/server.rs:88`（`form_secret`）、`:111`（`soft_file_limit`）、`:118`（`user_agent_suffix`）、`:167`（`max_image_resolution`）、`:243`（`warmup_pool`）
- **证据**：`max_image_resolution`、`warmup_pool`、`soft_file_limit` 全仓零点访问；`form_secret`、`user_agent_suffix` 仅出现在 `synapse-common/src/config/loader.rs:64,69-70` 的 env 解析管线（读入后无人消费）。
- **影响**：5 个运维可配项全部"配了不生效"。其中 `max_image_resolution` 与 `warmup_pool` 语义最危险——前者让人以为缩略图有分辨率上限，后者让人以为连接池有预热。
- **建议**：删除字段与 `loader.rs` 对应解析分支，同步删除样例配置与 `docs`；若 `max_image_resolution` 是待实现能力，则改为 `TODO(feature)` 记录而非静默接受。
- **不在此列**：`macaroon_secret_key` 有真实使用（`synapse-services/src/container.rs:378`、`wiring/e2ee.rs:61,77,144-162`），保留。

#### RD-13（P3）`LoggingConfig.log_dir` 只解析不使用
- **位置**：`synapse-common/src/config/loader.rs:91`（env 解析）；字段定义在 `server.rs` 的 `LoggingConfig`
- **证据**：除 `loader.rs:91` 与断言（`src/common/config/tests.rs:662`、`synapse-common/src/config/mod.rs:1050`）外无消费点；日志输出实际走 `tracing-subscriber` 的 writer。
- **影响**：配置 `log_dir` 后日志不会落到该目录。
- **建议**：删除字段；若确实需要文件日志，应显式实现 file appender 而非留一个空开关。

---

### 9.6 默认构建不可达的 feature 面

#### RD-14（P2）非默认 feature 存量：**5 个有独立实现文件的** feature 合计 4189 行，默认构建不可达
> **复核（2026-10-22 第二轮）**：原文标题写"7 个非默认 feature"，但下表只有 5 行——
> 这不是笔误而是口径混用：`voip-tracking` / `privacy-ext` 也有 `cfg` 门，但没有同等规模的
> 独立文件（改动分散在既有文件里），无法用 `wc -l` 计入。**标题改为"5 个有独立文件的"**，
> 4189 这个数字实测精确（逐文件 `wc -l` 相加）。
- **位置 / 规模**（`wc -l` 实测）：
  | feature | 主要文件 | 行数 |
  |---|---|---|
  | `server-notifications` | `synapse-services/src/server_notification_service.rs` | 1118 |
  | `builtin-oidc` | `synapse-services/src/builtin_oidc_provider.rs` | 1573 |
  | `saml-sso` | `synapse-web/src/routes/saml.rs` | 578 |
  | `cas-sso` | `synapse-web/src/routes/cas.rs` | 463 |
  | `voice-extended` | `synapse-web/src/routes/voice.rs` | 457 |
  | 合计 | | **4189** |
- **证据**：`Cargo.toml:29-64` 的 `default` 不含上述 feature；全仓 `feature = "(voice-extended|saml-sso|cas-sso|voip-tracking|server-notifications|privacy-ext|builtin-oidc)"` 共 **370 处 / 44 文件**（门控本身是到位的，不是"只到装配层"）。
- **影响**：并非"死代码"，但默认构建与默认测试**零覆盖**（TEST-06 已记录 5 个整文件被门控）；`check_feature_matrix.py` 尚未接入 CI（TEST-04），因此这些能力长期处于"存在但无人验证"的状态——这是本仓最大的"超前实现"存量。
- **建议**：二选一，不要继续悬空——① 在 CI 增加 feature 矩阵 job（至少 `--features all-extensions` 跑一次 `--no-run` + 单测）；② 对无落地计划的能力（`cas-sso`、`saml-sso`）显式标记为 experimental 并在 README 声明不支持。门禁必须可失败：故意破坏一个 feature-gated 模块的编译，矩阵 job 必须红。

---

### 9.7 始终失败但已注册的路由（预留扩展点）

> 共同特征：路由存在、鉴权通过、然后恒定返回错误。它们不是 bug（响应码符合契约），但属于"无业务价值的预留面"。**修复方向是"下架或登记"，不是"补实现"**。

#### RD-15（P3）第三方网络查询 4 个端点恒定 `M_UNRECOGNIZED`
- **位置**：`synapse-web/src/routes/thirdparty.rs:100`（`get_location`）、`:108`（`get_location_by_alias`）、`:131`（`get_user`）、`:139`（`get_user_by_id`）
- **证据**：四个 handler 直接 `Err(ApiError::unrecognized(...))`；文件 L88-93 注释自述"kept in the ledger so the route surface stays complete … mirrors the voice endpoints"。
- **影响**：客户端拿到"协议合法但永远失败"的响应，无法与服务端故障区分；每条请求仍要走鉴权中间件。
- **建议**：保留（若确有 bridge 客户端依赖该错误语义），但必须在 `ROUTE_CONTRACT.md` 单独登记为"by-design 不支持"，并纳入 §9.12 的清单守卫。

#### RD-16（P3）联邦 legacy key 4 个端点恒定 `M_UNRECOGNIZED`
- **位置**：`synapse-web/src/routes/federation/keys.rs:271`（`legacy_keys_upload`）、`:280`（`legacy_keys_claim`）、`:291`（`legacy_keys_query`）、`:303`（`query_auth`）
- **证据**：L265-306 注释说明原先 `query_auth` 返回 `{"auth_chain": []}` 会误导对端，故改为显式不可识别。
- **影响**：这些是 Synapse 早期联邦端点，现代对端不会调用；保留它们只为 404/501 语义差异。
- **建议**：与 RD-15 同批登记；若确认无对端依赖，直接从路由表移除（404 更诚实）。

#### RD-17（P3）`/rooms/{roomId}/event/{eventId}/keys` 恒返 501
- **位置**：`synapse-web/src/routes/handlers/room/events.rs:48-69`（返回点 `:68`）
- **证据**：handler 先做 `ensure_room_view_access` 再返回 `not_supported`；注释 C-1 说明"无密钥存储，原 200 `{"keys": []}` 会误导"。
- **影响**：每次调用都付出一次房间可见性查询的成本，只为返回一个恒定的 501。
- **建议**：保留鉴权（避免信息泄漏）但登记为 by-design；长期看应删除路由。

#### RD-18（P3）`update_report_score` 恒返 403，且带一个 `#[allow(dead_code)]` 字段
- **位置**：`synapse-web/src/routes/directory_reporting.rs:296-312`（`:299` 为 `#[allow(dead_code)] pub score`）
- **证据**：handler 只做 `validate_event_id` 后返回 `forbidden("Report score updates are not available via the client API")`。
- **影响**：请求体结构体仅为"能反序列化"而存在，字段永不读取；`#[allow(dead_code)]` 掩盖了这一点。
- **建议**：若端点保留，把 `score` 字段用于日志或直接删结构体改用 `Value`；若端点下架则一并删除。

#### RD-19（P3）`voice` 3 个不可用端点（**已自述 by-design**，仅登记不下架）
> 更正：原文标题写作"501 端点"，实测是 **404 `M_UNRECOGNIZED`**
> （`ApiError::not_implemented` 内部为 `ApiErrorKind::NotFound` + `MatrixErrorCode::Unimplemented`，
> 见 `synapse-common/src/error.rs:308-320` 的说明："Matrix defines no errcode for HTTP 501,
> so unimplemented endpoints are presented to clients as 404 M_UNRECOGNIZED"）。
- **位置**：`synapse-web/src/routes/voice.rs:373`（`convert_voice_message`）、`:384`（`optimize_voice_message`）、`:395`（`transcribe_voice_message`）
- **证据**：文件 L118-123 明确说明 MSC3245 把媒体处理放在客户端，三端点"intentionally return `M_UNRECOGNIZED`"；`get_voice_config`（`:125-147`）中的 `server_side_processing` 三字段硬编码 `false` 即该决策的声明。
- **影响**：无正确性风险；但路由表多出 3 个永不工作的入口，且 `get_voice_config` 里存在 `"max_duration_ms": 60_0000` 这种等价 600000 的怪异字面量（可读性）。
- **建议**：**不删**（属 MSC 合规声明），但把三端点与 `server_side_processing` 一起写入"有意不实现"清单；顺带把 `60_0000` 改为 `600_000`。

---

### 9.8 零引用 schema 对象

#### RD-20（P2）`notification_delivery_log` 只写不读
- **位置**：`migrations/00000000_unified_schema_v12.sql:3186`（建表）；`synapse-storage/src/server_notification/repository.rs:512-529`（唯一 `INSERT`）
- **证据**：全仓 `FROM notification_delivery_log` **0 命中**，仅有 INSERT。
- **影响**：表随时间无限增长且无任何读取方（无可观测性、无重试、无清理）；`server-notifications` 又是非默认 feature（RD-14），默认部署连写入都不会发生。
- **建议**：删除表与写入点，或补齐"查询投递历史"的消费者；若要保留审计价值，必须同时加 TTL/清理任务，否则是纯负担。

#### RD-21（P2）`user_locks` 表与 4 个存储方法在生产已无调用方
- **位置**：`migrations/00000000_unified_schema_v12.sql:4418`（建表）；`synapse-storage/src/user/storage.rs:1468-1550`（`lock_user`/`unlock_user`/计数/列表）
- **证据**：生产调用方**只有** `synapse-storage/src/test_mocks/mod.rs:53` 与 `db_tests.rs`、`user_store_fake.rs`；服务层 `user_lock_service` 已在"审查 #23"中删除（`synapse-services/src/wiring/extensions.rs:16-17` 自述）。
- **影响**：一张有写入语义（行锁）却无人调用的表，加一套完整的存储 API，构成"半删除"状态；后来者会以为存在并发用户锁机制。
- **建议**：按 `docs/synapse-rust/archive/REDUNDANT_TABLE_DELETION_PLAN.md` 的先例走"先删调用 → 观察窗口 → 删表 + 删方法"。

#### RD-22（P3）`users` 三个密码生命周期列生产零读取
- **位置**：`migrations/00000000_unified_schema_v12.sql:118-120`（`is_password_change_required`、`must_change_password`、`password_expires_at`）、索引 `:3319-3320`、初始化 `:4232`
- **证据**：全仓生产代码无点访问；唯一断言在 `tests/integration/user_storage_tests_migrated.rs:259-260`（断言列不存在/未被使用）。
- **影响**：三列 + 两个部分索引常驻，`:4232` 还会把 admin 置为 `must_change_password = TRUE`，但无任何登录逻辑消费它——形成"要求改密但永不生效"的静默行为。
- **建议**：二选一——实现强制改密流程（登录响应/`/account/password` 联动），或删除三列与 `:4232` 数据修补。后者更符合当前业务范围。

---

### 9.9 可简化但实现过繁的逻辑

#### RD-23（P3）`config/push.rs` 是 6 行纯 re-export 壳
- **位置**：`synapse-common/src/config/push.rs:1-6`
- **证据**：全文只有模块注释 + `pub use super::{ApnsConfig, FcmConfig, PushConfig, WebPushConfig};`；注释自述是 M-2 拆分 4081 行 `config/mod.rs` 的产物。
- **影响**：拆 god file 的方向正确，但产生了一个没有独立语义的空壳文件，增加"改一个类型要跳两层"的导航成本。
- **建议**：把 4 个结构体直接定义在本文件（真正完成拆分），或删除本文件、在 `config/mod.rs` 原地保留定义。同类壳文件应一次性普查。

#### RD-24（P3）`task_scheduler.rs` 111 行承载单个 `usize` 字段
- **位置**：`synapse-common/src/config/task_scheduler.rs:1-111`（字段 `:22`、自定义 deserializer `:36-57`、7 个测试 `:59-110`）
- **证据**：`TaskSchedulerConfig` 只有 `max_concurrent_tasks`；唯一业务读取点 `synapse-services/src/wiring/admin.rs:260`。上游对齐理由（#20230）写在 L7-12。
- **影响**：为 1 个整数维护 22 行反序列化 + 7 个测试，收益比偏低；`DEFAULT_MAX_CONCURRENT_TASKS` 的默认值校验可以用 `#[serde(default = "…")]` + 一行 `assert` 覆盖。
- **建议**：反序列化改用 `#[serde(default = "default_max_concurrent_tasks")]`，测试收敛为 2 条（默认值、非法值）。若上游配置段必须存在，保留结构体但去掉自定义 deserializer。

#### RD-25（P3）文件级 `#![allow(clippy::unused_async)]` 覆盖整个 handler 文件
- **位置**：`synapse-web/src/routes/e2ee/backup.rs:3`
- **证据**：文件头注释（`:1-2`）说明 stub handler 保持 async 签名；但该 `allow` 作用于**全文件**，未来新增的真异步/误异步函数都不会被检查。项目 clippy 基线为 `unused_async = deny`。
- **影响**：局部豁免被放大成文件级豁免，削弱了门禁。
- **建议**：改为逐函数 `#[allow(clippy::unused_async)]`，或在文件内用模块级 `mod stubs { #![allow(...)] }` 收窄作用域。

---

### 9.10 文档漂移（本轮新发现）

#### RD-26（P3）`API_COVERAGE_REPORT.md` 以已删除字段作为证据
- **位置**：`docs/synapse-rust/API_COVERAGE_REPORT.md:416`
- **证据**：该行写"`LivekitConfig.ws_url`（`config/voip.rs:92`）**全仓无读取点**"，但 `ws_url` 字段已被删除（`synapse-common/src/config/voip.rs:83-86` 有删除说明），现在 `:92` 是 `api_secret`。
- **影响**：读者按文档去指认"死配置"会看到无关字段；结论方向（Livekit 配置不生效）仍正确，但证据已失效。
- **建议**：改为"`LivekitConfig` 三字段（`api_key`/`api_secret`/`host`）整块无读取点"，与 MAINT-06 同步。

---

### 9.11 已复核并剔除的误报（不写入缺陷清单）

| 初筛结论 | 复核结果 | 剔除理由 |
|---|---|---|
| `room_summary_update_queue` 表零引用 | **假阳性** | 生产写入方 `synapse-services/src/room/messaging/messages.rs:409`、`room/messaging/events.rs:381,503,600`；消费方 `room/summary/stats.rs:68-96` |
| `sliding_sync_latency_threshold_ms` 零读取、能力声明误导 | **假阳性** | `synapse-services/src/sliding_sync_service/mod.rs:166` 注入，`:294` 真实比较并告警 |
| `delete_secure_backup` 是静默 no-op | **假阳性** | 实际调用 `secure_backup/service.rs:250-269` 的事务删除（BEGIN → DELETE → COMMIT） |
| `ProfileFlags::ALL` 硬编码 `saml_enabled: true` 是缺陷 | **假阳性** | `synapse-web/src/routes/route_module.rs:31-68` 注释明示 `ALL` 是"最宽 ceiling"，供离线工具/契约测试 |
| feature 门控只到装配层、实现体内无 `cfg` | **假阳性** | 全仓 `feature = "…"` 实测 **370 处 / 44 文件** |
| `macaroon_secret_key` 零读取 | **假阳性** | `synapse-services/src/container.rs:378`、`wiring/e2ee.rs:61,77,144-162` 在用 |
| `worker.stream_writers` 是纯声明面/超前实现 | **不列入** | 复核时已由 `bf15f173a`（W-I）落地：`worker.write_forwarding: true` 时六路真转发（typing/to_device/account_data/receipts/presence/push_rules），`events`/`device_lists` 配成非默认值**启动即报错**；默认仍关闭 ⇒ 零行为变化。属**已交付的一半**，剩余阶段见 `WORKER_STREAM_WRITE_FORWARDING_DESIGN_2026-10-22.md` §7 |
| `sync` 与 `sliding_sync` 是重复实现 | **改述** | 无复制粘贴函数体，属"共享语义基座缺失"（见 ARCH-02） |

---

### 9.12 统计与分阶段清理建议

| 分类 | 条目 | P2 | P3 | 小计 |
|---|---|---|---|---|
| 无用依赖 | RD-01…RD-04 | 1 | 3 | 4 |
| 空转门面 / 死 pub API | RD-05、RD-06、RD-27 | 2 | 1 | 3 |
| 测试替身进生产 | RD-07、RD-08 | 1 | 1 | 2 |
| 预留字段 / 容器死字段 | RD-09、RD-10 | 1 | 1 | 2 |
| 零读取配置 | RD-11…RD-13 | 2 | 1 | 3 |
| 非默认 feature 存量 | RD-14 | 1 | 0 | 1 |
| 恒定失败端点 | RD-15…RD-19 | 0 | 5 | 5 |
| 零引用 schema 对象 | RD-20…RD-22 | 2 | 1 | 3 |
| 可简化逻辑 | RD-23…RD-25 | 0 | 3 | 3 |
| 文档漂移 | RD-26 | 0 | 1 | 1 |
| **合计** | **27 条** | **10** | **17** | **27** |

**分阶段落地（每阶段都必须带可失败门禁）：**

| 阶段 | 范围 | 内容 | 门禁（必须能红） |
|---|---|---|---|
| 批次 1（零风险） | RD-02、03、04、05、06、23、24、25 | 删无用依赖、迁 dev-dependencies、删 storage 门面、删死访问器、并壳文件、收窄 clippy allow | `cargo build --workspace --all-targets` + `cargo test --workspace --no-run`；stage 人为把 `src/storage/mod.rs` 删掉后不得出现未解析引用 |
| 批次 2（需回归） | RD-01、07、08、09、10、11、12、13 | 根 crate 依赖清理、测试替身 `cfg` 门控、删容器预留字段、删零读取配置 | `cargo nextest run --workspace`；另加一条"配置字段存在性"测试（`deny_unknown_fields` 反向） |
| 批次 3（契约变更） | RD-15…RD-19 | 下架/登记恒定失败端点，同步 `ROUTE_CONTRACT.md` 与 `API_COVERAGE_REPORT.md` | `api_route_ledger_tests` + 契约文档与代码的路由集合比对脚本 |
| 批次 4（DB 迁移） | RD-20、21、22 | 删 `notification_delivery_log`、`user_locks`、`users` 三列，走"先停写 → 观察窗口 → 删表" | 迁移前备份；迁移后跑 `db_tests` 与 schema 快照对比 |
| 批次 5（能力决策） | RD-14 | feature 矩阵接入 CI，或显式声明 experimental | `check_feature_matrix.py` 必须接入并在故意破坏时失败 |

**不动的项**：RD-19（voice 三端点）属 MSC3245 合规声明，只登记不删除；RD-26 与 MAINT-06 合并为一次修复。

---

## 附录 A：已核实为"非缺陷"的项（避免重复排查）

| 项 | 结论 |
|---|---|
| 业务表 `SELECT *` | 0 处 |
| 4 个规则必需复合索引 | 4/4 存在（`access_tokens` 为 `WHERE is_revoked = FALSE` 部分索引，满足查询形态） |
| `events.content` GIN | 已建 `jsonb_path_ops` |
| `#[ignore]` 真实属性 | 16 处 / 7 文件，全部带理由，无"无理由 ignore"、无"引用未建 issue" |
| 主车道门禁（doc tests、`--workspace --lib`、unit/integration/e2e、insta assert-only、missing_docs ratchet） | 均为真门禁（可失败） |
| `dbg!` | 0 处 |
| `#[deprecated]` | 0 处 |
| `/metrics` 之外的管理面鉴权 | admin 299/299、appservice 12/12、worker 11/11 受保护 |
| `/_synapse/admin/v1/backups` 无路由 | 有意不注册（建议补进"有意不实现"清单，见 FUNC-05） |

> **复核补充（2026-10-22 第二轮）**：两个"时间窗"常数**属于不同校验器，不要互相代入**——
> `TIMESTAMP_WINDOW_SECONDS = 5*60`（`synapse-common/src/constants.rs:110`）用于
> `SecurityValidator::validate_timestamp`（`validation.rs:316`），而联邦签名时效走
> `federation.signing_ts_tolerance_ms`（默认 24h，`federation_auth.rs:201` →
> `validate_federation_timestamp`）。CODE-09 指的是前者的**注释**写成 "1 year" 与取值不符；
> SEC-07 指的是后者（24h）与重放缓存（300s）不匹配。两者都是真问题，但**不是同一个窗口**。

## 附录 B：本次审查未覆盖

> **复核更正（2026-10-22 第二轮）**：原文此处列了 7 个"并发会话未提交改动"并声明未阅读，
> 但正文 SEC-10 引用了其中 `synapse-services/src/url_preview.rs:104` 的精确行号——
> **声明与证据自相矛盾**。事实：那批改动已由 `c1ad6efdf` 提交，本轮复核时它们是 HEAD 的一部分，
> 因此按正文处理即可。下表为本轮补记的真实未覆盖项。

- **运行时数据**：仍未跑 benchmark / 压测（PERF-* 全部为静态推断；唯一实测是复核期间对
  `url_preview` 改动的定向用例，16/16）。
- **依赖版本分裂**：未重跑 `cargo tree -d`，沿用 `docs/synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md` 口径
  （注意该命令的输出**不能 `head` 截断后再下"唯一来源"结论**，历史踩过）。
- **全量集成套件**：未跑（本地全量约 2h，且需 `-j 2` 规避 PG 锁表；见 `scripts/run_full_integration.sh`）。
- **`events` 写分流的联邦面**：W-I 只覆盖六路，`events`/`device_lists` 的写路径**未纳入**本次审查
  （设计文档已列明阶段 2/3）。

## 附录 C：与历史清单的关系

| 历史文件 | 状态 |
|---|---|
| `docs/audit/UNRESOLVED_ISSUES_SUMMARY.md`（2026-10-09） | 已被本文件取代，建议标记 HISTORICAL |
| `docs/audit/REMAINING_ISSUES_SWEEP_2026-10-17.md` | 存在并发会话未提交改动，未纳入比对 |
| `docs/audit/TEST_DB_ISOLATION_DESIGN_2026-10-20.md` §8.6 | 4 条真 bug 已修并提交（`d3d41618b`），不计入本清单 |
| `docs/audit/ADMIN_RBAC_SURFACE_2026-10-21.md` | SEC-01 为其未闭环项，本次独立复现 |
| `docs/audit/DEPENDENCY_UPGRADE_BATCH2_2026-10-21.md` | 依赖治理进展，不在本次八维度口径内 |
