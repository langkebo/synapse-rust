# synapse-rust 全维度审查与结构化问题清单

> 审查日期：2026-10-22
> 审查基线：HEAD `d3d41618b`（工作区另有并发会话未提交改动，**未纳入审查、未触碰**）
> **复核基线：HEAD `bf15f173a`**（同日第二轮逐条复核；更正见 §0.1，被改写的条目正文带 `> **复核**：` 标记）
> **第三轮基线：HEAD `88554436b`**（同日第三轮；更正见 §0.2，新增 4 条缺陷，规划见 §十）
> **第四轮基线：HEAD `d86d825fd`**（次日复核；只核验 §10 各批次的落地情况 + 3 条更正，见 §0.3）
> 审查方式：静态审查（逐文件阅读 + 全仓 grep 计数 + 定点交叉复核）；**未运行完整测试套件**，因此除 §0.1/§0.2 明确标注"已修/已实测"者外，下文所有"行为"结论均为代码事实推断，不含运行时实测
> 计数口径：所有 grep 计数均以 `git ls-files` 的受版本控制文件为域（**排除 `target/`**）。直接 `grep -rn .` 会把构建产物算进去，历史踩过
> 严重度定义：
> - **P0**：导致数据丢失/服务不可用/认证绕过，须立即修
> - **P1**：破坏规范语义、产生不可见数据、或形成系统性安全/一致性问题，本迭代必修
> - **P2**：明确的正确性/性能/可维护性缺陷，有清晰修法，排期修
> - **P3**：规范漂移、文档失真、死代码，随手修

## 0. 总览

| 维度 | P0 | P1 | P2 | P3 | 小计 |
|---|---|---|---|---|---|
| 架构设计 | 0 | 1 | 4 | 1 | 6 |
| 代码质量 | 0 | 3 | 6 | **3** | **12** |
| 功能完整性 | 0 | **2** | 2 | 2 | **6** |
| 性能表现（含数据层） | 0 | 1 | **9** | **4** | **14** |
| 安全性 | 0 | 1 | 7 | 7 | 15 |
| 可维护性 | 0 | 0 | 4 | 4 | 8 |
| 测试覆盖 | 0 | 1 | 5 | 1 | 7 |
| 文档完整性 | 0 | 0 | 3 | 6 | 9 |
| **八维度小计** | **0** | **9** | **40** | **28** | **77** |
| 九、专项：冗余与过度开发 | 0 | 0 | 10 | 17 | 27 |
| **合计** | **0** | **9** | **50** | **45** | **104** |

> 加粗数字为第三轮新增（CODE-12、FUNC-06、PERF-13、PERF-14）。第三轮未新增 P0/P1 以外的高危项，但把 **CODE-02 的影响面从 5 处修正为 19 处**——结论等级不变，处置规模翻近 4 倍。

**结论摘要**：无 P0；9 条 P1 中有 4 条会直接造成"数据写进库但对客户端不可见/不可达"（ARCH-01、FUNC-03、FUNC-06、PERF-02），3 条是认证与缓存一致性（SEC-01、CODE-02、CODE-03），1 条是测试替身泄漏到生产 API（CODE-05），1 条是联邦信任根缺测试（TEST-01）。

**三条主干风险（按"修复后收益/成本"排序）**：

1. **缓存失效被静默吞掉**（CODE-02 / CODE-03，19+6 处）——一致性缺陷，现象偶发、难复现，但会让"已退房用户仍能读写"。修法单一、可机械替换，是本轮性价比最高的一项。
2. **配额与事务边界不可信**（FUNC-03 / FUNC-06 / PERF-02）——分片上传完全不计配额且可谎报大小；注册流程 5-6 次独立提交。前者是绕过，后者是"半注册"脏数据。
3. **Thread 影子表**（ARCH-01）——非规范 API 写入的数据对 `/sync`、`/messages`、联邦全部不可见，是唯一"写了等于没写"的结构性问题。

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

## 0.2 第三轮复核：改了哪些结论

第三轮的范围与前两轮不同：**不再逐条重读**（第二轮已覆盖），而是只做三件事——① 重跑全部"带数字/行号"的论断；② 对 P1 做"影响面是否被低估"的扩散扫描；③ 补做第二轮明确列为未覆盖的维度。

基线 `88554436b` 相对复核基线 `bf15f173a` 只多了两个提交（`369706979` SEC-10 修复、`88554436b` 文档复核），**代码主体未变**，故第二轮结论整体仍成立。以下只列**变化**：

| 条目 | 原结论 | 第三轮实测 | 处置 |
|---|---|---|---|
| **CODE-02** | P1：**5 处**静默吞掉 `room_state` 失效（仅 `membership/federation.rs`） | **⚠️ 严重低估**。生产路径 `let _ = …cache.delete(…)` 实测 **19 处**：`room_state:` **15 处**（`room/lifecycle/create.rs:558`；`room/membership/actions.rs:240,339,613`；`room/membership/federation.rs:480,531,748,1001,1110`；`room/membership/moderation.rs:123,183,488`；`room/messaging/events.rs:364,458,562`），另有 `account_data:` **3 处**（`account_data_service.rs:62,107`、`friend_room_service/mod.rs:683`）与 `user:profile:` **1 处**（`synapse-web/src/federation/edu.rs:785`） | 等级不变（P1），**影响面改述为 19 处**；修法从"改 5 行"升级为"建 helper + 全量替换 + 门禁" |
| **CODE-10** | P2：全仓 **1411 处 / 300 文件** | **⚠️ 口径错**。按 `git ls-files` 统计实为 **1593 处 / 337 文件**；剔除测试文件（`tests/`、`*/tests.rs`、`*_tests.rs`）后**生产代码 905 处 / 244 文件**。原数字既非全仓也非生产，来源不明 | 改为**双口径**：生产 905 / 全仓 1593，并给出可复现命令 |
| **ARCH-05** | P2：**7 个**文件 >2000 行 | ✅ 7 个仍成立，但清单不完整。另有生成文件 `synapse-web/src/routes/derived_route_table_always.inc.rs`（5476 行）与 3 个测试文件（`event/db_tests.rs` 3466、`test_isolation.rs` 3191、`sync_service/tests.rs` 2509）也超线 | 保留 7 个生产文件；补一句"生成文件与测试文件不计入拆分目标" |
| **TEST-07** | P3：实测 **277**（unit 126 + integration 146 + e2e 2 + perf 3） | **⚠️ 数字已漂移**。实测 **283**（unit **128** + integration **147** + e2e **3** + perf **5**） | 三处文档口径统一改为 283，并给出分项 |
| 附录 A `#[ignore]` | "16 处 / 7 文件" | **⚠️ 口径混入**。裸 `grep '#\[ignore'` 得 34 处，但其中多数是 `e2e_honesty_tests.rs` / `ci_test_scope_tests.rs` 里的**字符串与扫描器断言**（它们本身就在禁止裸 ignore）。真实属性为 **11 条**：`tests/e2e/user_flow_tests.rs` 7 条、`tests/performance/*` 4 条，**全部带理由** | 改为"11 条真实属性，全部带理由"；字符串匹配不计入 —— **⚠️ 第四轮复核推翻此更正，实为 16 条 / 7 文件，见 §0.3** |
| **FUNC-03** | P1：完成时分片上传绕过 `record_upload_usage` | ✅ 成立，且**比原描述更严重**——见新增 **FUNC-06**：`total_size` 为 `None` 时连预检都不做，且该值由客户端声明、与实际大小无关 | 保留 FUNC-03；新增 FUNC-06 覆盖"预检与上界"两条独立缺口 |
| **SEC-01** | P1：RBAC 关闭时超管黑名单失效 | ✅ 复现成立（`admin_auth.rs:66-67` 短路 `allowed`，黑名单在 `is_role_allowed` 内部） | 保留 |
| **SEC-04** | P2：未设 `PROMETHEUS_AUTH_TOKEN` 时 `/metrics` 匿名 | ✅ 成立（`src/server/mod.rs:1091` 显式 `.filter(\|s\| !s.is_empty())`，空值即关闭鉴权） | 保留 |
| **SEC-03** | P2：MFA 只校验"非空" | ✅ 成立（`validation.rs:17` 仅判 `is_empty()`，无占位符/长度/Base32 校验） | 保留 |
| **SEC-10** | P3，已修 | ✅ 已修确认：`loggable_url()` 只留 `scheme://host` + ≤64 字符 path（按 char boundary 截断），query/fragment 全丢；用例 `logged_url_drops_the_query_and_caps_the_path` 在库 | 关闭 |
| **RD-20 / RD-21** | P2：`notification_delivery_log` 只写不读；`user_locks` 生产零调用 | ✅ 成立。前者全仓唯一 `INSERT` 在 `server_notification/repository.rs:514`，无任何 `FROM`；后者仅 storage 自身方法 + 测试夹具 | 保留 |
| **PERF-12** | P3：`INDEXES.md` 342 vs SQL 347 | ✅ 成立，且 347 个 `CREATE INDEX` **索引名互不重复**（不是重复计数） | 保留 |

**第三轮新增 4 条**（详见正文）：

| 编号 | 级别 | 一句话 |
|---|---|---|
| CODE-12 | P3 | `preview.rs:71` 有一句无副作用的死表达式，每次请求白算一次时间戳，并误导读者以为它是缓存键 |
| FUNC-06 | **P1** | 分片上传的 `total_size` 由客户端声明：可为 `None`（跳过预检），也可谎报（预检失效）；实际大小只在合并后才校验 |
| PERF-13 | P3 | URL 预览首跳被解析两次（调用方 `check_url_and_resolve` + service 内 `fetch_html`），每次预览多一次 DNS |
| PERF-14 | P2 | `load_completed_upload` 先 `extend_from_slice` 拼完全部 chunk，**之后**才比 `max_file_size`，峰值内存 ≈ 文件大小（最大可到配额上限） |

**第三轮明确"不改结论"的部分**：P1 的其余 7 条、全部 P0=0 判定、§9.11 剔除的 9 条误报，均未发现新反证。附录 B 的"未覆盖项"仍然有效（本机未跑全量套件与 benchmark）。

---

## 0.3 第四轮复核：批次落地核验（2026-10-23）

这一轮**不重读缺陷**，只回答一个问题：§10.1 声称"已完成"的批次，在 HEAD 上是否真的成立。
方法是逐批回代码/门禁取证（文件行号、脚本退出码、用例计数），不采信本文件的自我陈述。

| 批次 | 本文件 | 实测判定 | 关键证据 |
|---|---|---|---|
| W0 `8b8505ff5` | ✅ | **成立** | `let _ = …cache.delete` 残留 0；`check_no_silent_cache_delete.sh` exit 0 且接 `ci.yml:139`；墓碑 `get_raw` 只读 L1（`manager.rs`）；`-p synapse-cache --lib` 95/95 |
| W1 `cc364bd6f` | ✅ | **成立** | `media/mod.rs:386` `missing_param` + 超限 413；`complete_chunked_upload` 走 `upload_media_with_id_accounted(…, Some(upload_id))`；`create_registration` 真事务；契约 E.17 在位 |
| W2 `85180061c` | ✅ | **成立** | `admin_auth.rs:209-225` 顺序＝超管→黑名单→开关→角色；`user_store_fake` 的 `mod`/`pub use` 双门禁、`unimplemented!`=0；`test01_*` 4/4 |
| W3 `3789583ef` | ✅ | **成立** | `thread_service` → `RelationsService::send_reference`（`m.thread`），根事件缺失 404；E.18 在位 |
| W4 `2358c0f5a` | 🟡 第一层 | **成立**（第二/三层随后由 `0d18fede0` / `3cf5c006a` 补齐） | 6 处 `ILIKE` + `POSITION(...)>0`；`prune_in_batches` + `PRUNE_BATCH_SIZE=1000`；`explain_check.sh` 接 `schema-health-check.yml:82` |
| W5 `ca6980c51` | ✅ | **代码成立，门禁当时是红的** | 三态 `PrometheusAuth`、`validate_admin_mfa_secret`、窗口从容差推导、`:?` 必填均实；但 `check_no_default_admin_credential.sh` **自匹配自身注释 ⇒ exit 1**，见下 |
| W6 | 待开始 | **一致**（本文件未虚报） | `preview.rs:71` 死表达式、`one_time_invite_service.rs:139`、`dm.rs` 的 `internal` 均在；`check_doc_metrics.py` 不存在 |

**P1 = 0 的自述成立**：9 条 P1 全部有实现 + 能失败的门禁（上表 W0–W3 覆盖 8 条，TEST-01 4/4）。

**本轮更正 3 条**（前两条是"把正确的数字改错"，方向与前几轮相反）：

| 条目 | 本文件现有说法 | 实测 |
|---|---|---|
| 附录 A / §0.2 `#[ignore]` | 11 条（第三轮把 16/7"更正"成 11） | **16 条 / 7 文件**。第三轮只数了 `tests/e2e` + `tests/performance`，漏掉 `tests/unit/pagination_gate_tests.rs`×2、`admin_auth/admin_rbac_tests.rs`×1、`synapse-e2ee/src/{crypto/aes.rs,ssss/service.rs}` 各 1 —— 即第二轮的 16/7 本是对的 |
| PERF-01 | "该表没有任何保留/清理任务"（取证 `grep -rn to_device_messages synapse-services/src src/`） | 取证命令跳过了 `synapse-storage`：`dehydrated_device.rs:153,184` 是生产路径的 `DELETE FROM to_device_messages`（脱水设备删除/过期）。结论应为"**普通设备**无按龄清理" |
| §10.7 门禁表 SEC-02 行 | `check_no_default_admin_credential.sh` "OK" | **自 `ca6980c51` 起恒红**：脚本自己第 5/22 行注释写了默认口令字面值，扫描面无自我排除；`PATTERN='Admin@12[3]'` 只护住模式那一行。**已修（`d86d825fd`）**：注释改为不含字面值的表述 + 在 `PATTERN` 上方写明该纪律；修后 exit 0，变异自证（注入真默认值 → exit 1 → 删除 → exit 0）通过 |

**口径漂移（可复现，属正常老化）**：`#[allow]` 268→**270**/117；CODE-10 全仓 1593→**1573**/338、
生产 905→**882**/243；测试文件 283→**284**（unit 128→129，147/3/5 未变）；`CREATE INDEX` 347→**349**。
§10.11 的"数字可复现"仍然成立——变的是代码，不是口径。

**方法教训**：`git show <sha>:<path>` 在 zsh 里会被当成 `:s` 修饰符而静默返回空，
本轮据此一度误判"该字面值从未进过脚本"。追溯单文件历史要写 `git show HEAD:<path>` 或引号包裹。

---

## 一、架构设计

### ARCH-01（P1）Thread 双实现：非规范 API 写入影子表，数据对 `/sync`、`/messages`、联邦不可见
> **修复（2026-10-22，W3 方案 A，提交 `3789583ef`）**：采用下方"最小修法"——vendor thread API
> 改为写真实 `m.thread` 事件（经 `RelationsService::send_reference`），影子表降级为索引；
> 建线程要求根事件真实存在（否则 404）。存量影子行未回填，详见 §10.5 落地状态。
>
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
- **范围边界（第三轮补充）**：另有 4 个文件也 >2000 行，但**不列入拆分目标**——`synapse-web/src/routes/derived_route_table_always.inc.rs`（5476 行，生成文件，改生成器而非改产物）；测试文件 `synapse-storage/src/event/db_tests.rs`（3466）、`tests/…/test_isolation.rs`（3191）、`synapse-services/src/sync_service/tests.rs`（2509）。

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

### CODE-02（P1）生产路径 19 处静默吞掉缓存失效（`room_state` 15 / `account_data` 3 / `user:profile` 1）
> **第三轮更正（2026-10-22）**：原文只统计了 `membership/federation.rs` 的 5 处，**低估了近 4 倍**。
> 全量扫描 `let _ = …cache.delete(…)` 得 19 处，其中真正影响鉴权的 `room_state:` 有 15 处，
> 分布在 6 个文件——即"联邦面修完、本地面和消息面照旧"的半修复状态，比"全都没修"更难发现。
- **位置（`room_state:` 15 处）**：`room/lifecycle/create.rs:558`；`room/membership/actions.rs:240,339,613`；`room/membership/federation.rs:480,531,748,1001,1110`；`room/membership/moderation.rs:123,183,488`；`room/messaging/events.rs:364,458,562`
- **位置（其他前缀 4 处）**：`account_data_service.rs:62,107`、`friend_room_service/mod.rs:683`（`account_data:`）；`synapse-web/src/federation/edu.rs:785`（`user:profile:`）
- **类型**：错误吞噬 / 一致性
- **影响范围**：本地建房间、成员变更、消息写入、审核操作都会在删除失败后继续用旧 `room_state` 判定成员资格与事件鉴权，最长到 TTL 到期；期间可能对已退房用户放行、对已加入用户拒绝。`account_data` 失效丢失则表现为跨设备已读/标签不同步。
- **修法**：引入单一 `invalidate_room_state(&cache, &room_id)`（内部 `warn!` + 本地短 TTL 墓碑 + 失败重试队列），**全量替换 19 处**；再加一条 clippy `let_underscore_must_use` 或 CI 扫描禁止对 `cache.delete` 使用 `let _ =`。分批推进：先 15 处 `room_state`，再 4 处其他前缀。
- **状态：W0 已修（2026-10-22）**。实现与本条原写法不同：`CacheManager::delete` 返回 `()` 且内部已 `warn!`，所以问题不在吞错，而在 **L2 DEL 失败 → 下次读从 L2 回填把旧值复活**；修复落在 `synapse-cache/src/manager.rs` 的墓碑机制（实例字段、无 TTL、只拦 L2 读）。services 侧按 key 收敛为 2 个 helper（`room_state_cache::invalidate_room_state`、`account_data_service::invalidate_account_data`），19 处全部替换；`synapse-web/src/federation/edu.rs:785` 只有一处删除点，直接去掉 `let _ =`。门禁 `scripts/ci/check_no_silent_cache_delete.sh` 已接入 CI，红/绿均实测。详见 §10.2。

### CODE-03（P1）设备删除路径 6 处吞掉懒加载成员清理失败
- **位置**：`synapse-storage/src/device/mod.rs:494,634,673,730,770,794`
- **类型**：错误吞噬
- **影响范围**：设备/用户删除后残留 `lazy_loaded_members` 行，`/sync` 会把这些成员当作"客户端已知"而抑制状态事件，表现为客户端看不到本应出现的成员变更。D-37 只修了 `:566` 一处，同类问题仍在。
- **修法**：与 `:566` 同样的处理方式统一 6 处；并给该族函数加"返回值必须被处理"的 lint（`must_use`）。
- **状态：W0 已修（2026-10-22）**。按"事务内 / 已提交"分流：事务内的 `:494`、`:673` 改为 `.await?` 向上传播（事务回滚，不留半删状态）；事务已提交的 `:634`、`:730`、`:770`、`:794` 回滚无意义，改为 `warn!` 并带 `user_id`/`device_id`，与 `:566` 的 D-37 措辞一致。`must_use` lint 未加（这些函数在测试与事务内均有合法忽略场景，收益低于噪声）。详见 §10.2。

### CODE-04（P2）邀请认领时静默丢弃房间配置
- **位置**：`synapse-services/src/one_time_invite_service.rs:139`
- **类型**：错误吞噬
- **影响范围**：`serde_json::from_value(claimed.room_config).unwrap_or_default()` 在配置结构演进/字段不匹配时退化为默认配置，房间以错误参数创建且无任何日志。
- **修法**：`map_err` 成显式错误（或 `warn!` + 保留原始 JSON 落库以便重放）。
- **状态：已修（2026-10-22，W4 第三层）**。`unwrap_or_default()` 换成 `parse_stored_room_config`：反序列化失败返回 `internal_with_context`（500 + cause），不再退化为默认配置。配套补 `release_claim_after_failure`——认领发生在解析**之前**，若不释放，一条坏 blob 会把邀请永久钉死在 claimed 状态；释放失败只 `error!` 记录，不掩盖原始错误（令牌仍可用，修好 blob 后可重试）。用例 `stored_room_config_that_no_longer_decodes_is_an_error_not_a_default`（坏形态 → 500，好形态仍可解码）。

### CODE-05（P1）`FakeUserStore` 的 `unimplemented!` 未做测试门禁即对外导出
- **位置**：`synapse-storage/src/user_store_fake.rs:64`；导出点 `synapse-storage/src/lib.rs:160-161,230`
- **类型**：测试替身泄漏到生产 API
- **影响范围**：任何下游误用 `FakeUserStore` 都会在运行时 panic；且该类型是公开 API 的一部分，破坏了"生产 crate 不暴露测试替身"的边界。
- **修法**：`#[cfg(any(test, feature = "test-utils"))]` 门禁 + 未实现的方法返回 `Err` 而非 `unimplemented!`。
- **状态：已修（2026-10-22，W2）**。`user_store_fake` 的 `mod` 与 `pub use` 同时门禁；伪造实现改为 `Err("not implemented in FakeUserStore")`。默认 feature 构建不含该符号。

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
- **状态：已修（2026-10-22，W4 第三层）**。`m.direct` 的解码抽成纯函数 `direct_map_from_account_data`：`Some(对象)` → 原样返回，`Some(非对象)` → `bad_request("Invalid m.direct account data format")`（`M_INVALID_PARAM`），`None` → 空 map（"从未设置"不是客户端错误）。同类站点一处：`friend_room_service` 的 `Some(_)` 分支原为 `Internal`，一并改为 `FriendRoomError::InvalidInput`。用例 `test_direct_map_from_account_data_maps_non_object_to_bad_request`。

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

### CODE-10（P2）生产 905 处 / 全仓 1593 处 `let _ =` / `if let Ok(` / `unwrap_or_default()`，无收敛策略
> **第三轮更正（2026-10-22）**：原文"1411 处 / 300 文件"**口径不明且不可复现**。按受版本控制文件重测：
> 全仓 **1593 处 / 337 文件**；剔除测试文件（`tests/`、`*/tests.rs`、`*_tests.rs`）后生产代码
> **905 处 / 244 文件**。差异主要来自是否把 `target/` 与测试代码计入。
- **位置**：全仓；复现命令见下
- **类型**：错误吞噬面
- **影响范围**：CODE-02/03/04 都是这一模式的实例；没有 lint 或棘轮，新增静默失败无法被拦截。但**不能用"总数"做验收**——其中大量是 `unwrap_or_default()` 处理 `Option` 的合法用法，逐个清理会制造噪音。
- **修法**：按**危险性**而非数量治理：① 对 `cache.delete` / `storage.delete` / `mark_finalized` 这类"失败即状态不一致"的调用，禁止 `let _ =`（CI 扫描 + 白名单）；② 对 `#[must_use]` 返回值启用 clippy `let_underscore_must_use`；③ 总数只做趋势记录，不设下降配额。
- **复现**：

```bash
# 全仓（1593 处 / 337 文件）
git ls-files -z '*.rs' | xargs -0 grep -cE 'let _ =|if let Ok\(|unwrap_or_default\(\)' | awk -F: '{s+=$2} END {print s}'
# 生产代码，剔除 tests/、*/tests.rs、*_tests.rs（905 处 / 244 文件）
git ls-files '*.rs' | grep -vE '(^|/)tests?/|/tests?\.rs$|_tests\.rs$|^tests/' \
  | xargs grep -hcE 'let _ =|if let Ok\(|unwrap_or_default\(\)' | awk '{s+=$1} END {print s}'
```

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

### CODE-12（P3，第三轮新增）URL 预览 handler 里有一句无副作用的死表达式
- **位置**：`synapse-web/src/routes/media/preview.rs:71`
- **证据**：
  ```rust
  let _ = ts.unwrap_or_else(current_timestamp_millis);
  ```
  `ts` 是上游传下来的 `Option<i64>`；无论 `Some/None`，结果都被丢弃。这行每次请求都会在
  `None` 分支调用一次 `current_timestamp_millis()`，然后什么都不做。
- **类型**：死代码 / 误导性代码
- **影响范围**：无功能影响（不产生错误行为），但会让读者误以为它是缓存键或时间戳校验；
  这类"看起来在做事"的语句比明显的死代码更有害——下一个改这里的人会以为删掉它会改变行为。
- **修法**：直接删除该行；若原意是做缓存旁路，则改为显式 `if let Some(ts) = ts { ... }` 并补测试。

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
- **影响范围**：所有走分片上传的媒体不计入用户/房间配额，配额限制可被绕过；与整块上传的记账口径不一致。整块上传路径 `upload_media_with_id` 是 `ensure_upload_allowed` → 上传 → `record_upload_usage`，分片路径**只做了第一步的一部分**。
- **修法**：在分片完成路径复用 `record_upload_usage`；更好的做法是把"记账"下沉到 `upload_media_with_id` 内部（用参数区分"已记账/待记账"），消除调用方分歧——否则下一条新增上传路径还会漏。
- **回归用例**：分片上传 N MB 后，断言 `quota_used` 增加 N；再继续上传直到超限，断言被 `ensure_upload_allowed` 拒绝。

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

### FUNC-06（P1，第三轮新增）分片上传的 `total_size` 由客户端声明，配额预检可被完全跳过
- **位置**：`synapse-services/src/media/mod.rs:337-346`（`start_chunked_upload`）、`synapse-services/src/media/chunked_upload.rs:83-140`（`start_upload` / `upload_chunk`）
- **类型**：配额逃逸 / 输入不可信
- **证据**：
  ```rust
  // media/mod.rs — total_size 为 None 时整段跳过预检
  if let Some(size) = total_size {
      if size > 0 { self.ensure_upload_allowed(user_id, size).await?; }
  }
  // chunked_upload.rs — upload_chunk 只校验单块 ≤ chunk_size_limit(10MB)
  if chunk.len() > self.chunk_size_limit { return Err(...) }
  // 不校验 uploaded_size + chunk.len() 是否超过 total_size，也不校验是否超过 max_file_size
  ```
- **影响范围**：与 FUNC-03 是**两条独立缺口**——FUNC-03 是"上传完了不记账"，本条是"上传前不拦住"。
  ① 不传 `total_size` ⇒ 完全不预检；② 传 `total_size = 1` 而实际传 1 GiB ⇒ 预检通过，
  累计大小直到 `load_completed_upload` 才用 `max_file_size` 拦截（此时磁盘与内存已被占用）；
  ③ 两者叠加后，配额形同虚设。
- **与 FUNC-03 的关系**：FUNC-03 修好（记账）后本条仍然成立（超额已发生，记账只是事后统计）。
- **修法**：① `total_size` 改为**必填**，或服务端在 `start_upload` 时用 `total_size` 预占配额（reservation）；② `upload_chunk` 增加累计校验：`uploaded_size + chunk.len() > total_size.max_limit` 立即 413；③ 完成时若实际大小与 `total_size` 偏差超过阈值，记 `warn!` 并按实际值调整配额。
- **回归用例**：`total_size=None` 时必须 400；声明 1 KiB 传 2 KiB 时第 2 块即失败；声明的值超 `max_upload_size` 时 `start` 即失败。

---

## 四、性能表现（含数据层）

### PERF-01（P2）`to_device_messages` 只增不减；其"全量读取"API 是死代码
> **复核（2026-10-22 第二轮）**：原判 P1 的主结论**不成立**——`/sync` 并不全量读取。
> `get_to_device_events` → `get_messages_since(user, device, since_stream_id, limit)`，
> SQL 为 `WHERE stream_id > $3 … LIMIT $4`（`synapse-e2ee/src/to_device/storage.rs:365-390`），
> 上限来自 `performance.sync_to_device_limit`。被原文点名的 `get_messages`（`:332`，无 LIMIT）
> 与 `get_and_delete_messages`（`:461`）**全仓零调用方**（见 RD-27）。
> 仍然成立、且更值得修的是**另一半**：该表没有按龄（retention）清理。
> **复核（第四轮，2026-10-23）**：第二轮的"没有任何清理代码"**说过头了**，取证命令也把范围写窄了
> —— `grep -rn to_device_messages synapse-services/src src/` 恰好跳过了 `synapse-storage`。生产路径上
> **有**两处 `DELETE FROM to_device_messages`：`synapse-storage/src/dehydrated_device.rs:153`
> （`delete_by_user`）与 `:184`（`sweep_expired`），都只在**脱水设备**被删除/过期时清理该设备的行。
> 所以准确表述是"**普通设备**的队列无按龄清理"，第二轮的结论方向不变、影响面收窄。
- **位置**
  - 无按龄清理：`synapse-storage/src/pruning.rs` 有 10 个 `prune_*`，其中 `prune_old_to_device_transactions`
    清的是**去重事务表**，**没有**针对 `to_device_messages` 的条目；唯一的清理是脱水设备删除/过期时的
    顺手删除（`dehydrated_device.rs:153,184`，见上方第四轮复核）
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

> **落地状态（2026-10-22，W4 第一层已实现，提交 `2358c0f5a`）**。三条的修法都与原文不同：
>
> | 条目 | 实际做法与偏差 |
> |---|---|
> | PERF-03 | 选「查询去掉 `LOWER()`」而非「索引改 `LOWER(name)`」。原因：`CREATE INDEX IF NOT EXISTS` 对**同名**索引不会替换定义，已在真实库迁移过的 `idx_rooms_name_trgm` 会静默保持旧定义，改索引在线上等于没改。搜索词在 service 层已统一 `to_lowercase()`（`search_service.rs` 三处 `format!("%{}%", term.to_lowercase())`、`admin/room/mod.rs` 一处），故 `col ILIKE $1` 与原写法**语义等价**，且只有它能用上建在原始列上的 trgm 索引。另补 `idx_rooms_topic_trgm` —— 两处查询都有 `topic` 谓词，但当时只有 name/canonical_alias 两条索引 |
> | PERF-04 | 原建议的 `content @> ...` 与 `tsvector` 都**不适用**：`@>` 是键值包含、不是子串匹配，满足不了 `search_term` 的语义；`tsvector` 会改掉检索语义（分词 + 语言依赖）。落地改为 `content::text ILIKE $2` + 表达式索引 `idx_events_content_trgm`（`GIN ((content::text) gin_trgm_ops)`）。原 `jsonb_path_ops` 保留（它服务的是 `@>`，两者不互相替代） |
> | PERF-05 | **影响面判断偏高**：行集本已被 `(room_id, membership)` 收窄并走 `idx_room_memberships_joined`，`LIKE '%:%'` 只是行内 `textlike` 过滤，不存在"全表/全索引扫"。改动价值在于去掉前导通配的模式匹配与转义路径，并把"含冒号"这一意图写显式 |
>
> **门禁**：新增 `scripts/quality/explain_check.sh`，双层断言——源码形态 4 条（防"代码改回去而索引仍在、计划断言仍绿"）+ 计划 5 条（`idx_rooms_{name,topic,canonical_alias}_trgm`、`idx_events_content_trgm`、`idx_room_memberships_joined`）。已接入 `schema-health-check.yml`（复用该车道刚灌好的库，`EXPLAIN_CHECK_EXISTING_DB=1`）。**PERF-04 的门禁必须额外关 `enable_indexscan`/`enable_indexonlyscan`**：空表下 `idx_events_sync_covering`（Index Only Scan，不回表）代价低于 trgm 的 Bitmap Heap Scan，规划器会先选它——那是代价差异、不是索引不可用。
>
> **实测证据**：门禁红态 = 改回 `LOWER(...) LIKE` 时源码断言即 FAIL，且计划退化为 `Seq Scan on rooms`；绿态 = `Bitmap Index Scan on idx_rooms_name_trgm` / `idx_events_content_trgm`。测试面 `api_public_rooms_tests` 3 passed、`api_search_thread_tests` 15 passed、`cargo test -p synapse-storage --lib search_` 36 passed。
>
> **`.sqlx` 连带**：4 条宏查询文本变更 ⇒ 1334 → 1330（删 8 增 4），其中 4 条是**既有孤儿**（旧宏版 `search_room_messages_admin` + 3 条 `push_rules`），此前只是没人跑过写模式。

### PERF-06（P2）10 个 prune 全部无分批；保留策略单条删除
> **复核（2026-10-22 第二轮）**：条目数 ✅（实测 10 个 `prune_*`），但**原文路径不存在**：
> `synapse-services/src/tasks/pruning.rs` 无此文件（`synapse-services/src/tasks/` 目录本身不存在），
> 实际位置是 `synapse-storage/src/pruning.rs:99-245`。
- **位置**：`synapse-storage/src/pruning.rs`（10 个 prune 入口）；`synapse-services/src/admin/retention.rs:264-280`
- **类型**：长事务 / 锁放大
- **影响范围**：首次清理或积压后单条 `DELETE` 形成长时间锁与巨量死元组，期间写入抖动、复制延迟上升。
- **修法**：统一 `DELETE ... WHERE id IN (SELECT id ... LIMIT :batch)` 循环 + `VACUUM` 触发。
- **状态：已修（2026-10-22，W4 第三层）**。新增 `PRUNE_BATCH_SIZE = 1000` 与 `prune_in_batches`，10 个 `prune_*` 统一为 `DELETE … WHERE ctid IN (SELECT ctid … LIMIT $n)` 轮询（`id` 不通用：`presence`/`one_time_keys` 等表并非以 `id` 为唯一键，`ctid` 对整表成立）。返回值仍是**跨批总数**，调用方日志语义不变。`retention::delete_local_messages_before`（管理端保留策略的单条删除）同样改为分批，返回 `events_deleted` 总数；中途打断只会留下部分已过期行，重跑继续。**未实现"每批 sleep 可配"**：限流由批大小承担，后台任务先做完再睡只会拉长积压。两条零 DB 用例锁契约（`prune_batches_never_ask_for_more_than_the_batch_size` 断言每批 ≤ batch_size 且返回总数、`prune_batches_stop_on_an_empty_backlog`）+ 一条真库用例 `prune_drains_a_backlog_larger_than_one_batch`（播 `PRUNE_BATCH_SIZE + 1` 行，断言全删且计数为总数）。

### PERF-07（P2）3 处列表查询无 `LIMIT`
- **位置**：`synapse-services/src/media/chunked_upload.rs:294,302`、`synapse-services/src/invite_blocklist.rs:150`
- **类型**：无界查询
- **影响范围**：单用户/单房间的积压数据直接决定响应体大小与内存占用，可被构造为内存放大攻击面。
- **修法**：强制 `LIMIT` 上限并从配置读取。

> **落地状态（2026-10-22，W4 第二层已实现，提交 `0d18fede0`）**。原文 3 处全部复核，但只有 1 处真正需要"加 `LIMIT`"：
>
> | 原站点 | 复核结论 | 处置 |
> |---|---|---|
> | `chunked_upload::load_chunk_data` | ✅ 无界，且行数由客户端申报的 `total_chunks` 决定 | 加 `LIMIT`；`start_upload` 侧把 `total_chunks` 夹在 `1..=MAX_CHUNKS_PER_UPLOAD(10_000)`，越界返回 `M_TOO_LARGE` |
> | `chunked_upload::list_expired_upload_ids`（原 `cleanup_expired` 内联） | ✅ 无界 | 提到独立方法并加 `LIMIT`，`cleanup_expired` 改为**分页循环**（每页 `EXPIRED_UPLOAD_CLEANUP_PAGE=100`），否则加了 `LIMIT` 只会漏删 |
> | `invite_blocklist.rs:150`（全局邀请名单） | ❌ 该函数**没有调用方** | 删死代码优于给它加一个没人用的 `LIMIT`；同时删掉同样无人调用的 `list_user_uploads` |
>
> 两点实现取舍：
>
> - **"拒绝"而不是"截断"落到了读侧**：`load_completed_upload` 取 `total_chunks + 1` 行，多出一行即说明有越界写入，返回 500 而非静默拼出半截文件。`start_upload` 的上限只约束诚实客户端，读侧这道才是兜底。
> - **分页循环的终止条件**：某页一条都没删掉即 `break`（否则同一页会被反复取回形成死循环），短页即视为排空。
>
> 测试：storage 3 个新 db_test（行数上限 / 批次上限 / 跨页排空，后者临时把 `loop` 降为单轮可复现 100 vs 101 的红）+ services 3 个（超上限 413、小于 1 返回 400、读侧越界 500）。`.sqlx` 1330 → 1327（删 5 增 2），`check_sqlx_cache_fresh.sh --compile` 通过。

### PERF-08（P2）5 处 OFFSET 深分页
- **位置**：`synapse-storage/src/user/storage.rs:1543-1558`、`synapse-storage/src/invite_blocklist.rs:386,419`、`synapse-storage/src/sliding_sync/repository.rs:301`、`synapse-web/src/routes/handlers/room/events.rs:568-593`（先取 `from+limit` 再内存切片）
- **类型**：深分页退化
- **影响范围**：页码越深越慢，且内存切片版会在服务端把已跳过的行也读出来。
- **修法**：改 keyset（`WHERE (ts, id) < (:ts, :id)`）分页；`events.rs` 直接用 SQL 的 `OFFSET/LIMIT` 或游标。

> **落地状态（2026-10-22，W4 第二层已实现，提交 `0d18fede0`）**。5 处逐点定性，**只有 1 处真的退化，其余是既有契约**：
>
> | 站点 | 定性 | 处置 |
> |---|---|---|
> | `room/events.rs`（`get_room_encrypted_events`） | ✅ 真问题：`fetch_limit = offset + limit + 1` 全量读回再内存 `skip/take` | 窗口下推 SQL：`LIMIT $3 OFFSET $4`，只取 `limit + 1` 行（多 1 行判 `has_more`） |
> | `sliding_sync/repository.rs:301` | ⚠️ 位置窗口是 MSC3575 的 `range: [start, end]` 语义，不能换成 keyset | `OFFSET` 保留；**补 `room_id` 决胜键**（原 `ORDER BY bump_stamp DESC` 并列时跨页顺序不定） |
> | `invite_blocklist.rs:382,415` | ❌ 不是问题：已是 `*_paginated(limit, offset)`，`limit`/`offset` 是 admin API 的 **wire 参数**且响应回显 | 保持不变（两处排序 `created_ts DESC, user_id ASC` 已是全序） |
> | `user/storage.rs:1670` | ❌ 不是问题：`get_locked_users(limit, offset)` 是 admin 分页接口，`user_locks` 有"每用户至多一条 active"的天然上限 | 保持不变（`created_ts DESC, id DESC` 已是全序） |
>
> 三点与原文不同的判断：
>
> - **没有改 keyset**。该端点 `from` 的契约在 handler 的 doc comment 里写明是 "plain offset"，改 keyset 会换掉 `next_batch` 语义（原文也承认"这是契约变更"）。同样的收益（不再传输被跳过的行）用 `OFFSET` 下推即可拿到，且 wire 契约零变更。
> - **`OFFSET` 要求排序是全序**，这一点原文没写。原 `get_room_events_by_type` 只有 `ORDER BY origin_server_ts DESC`（毫秒并列时不稳），补成 `origin_server_ts DESC, stream_ordering DESC NULLS LAST, event_id DESC`；`sliding_sync` 同理补 `room_id`。**不补的话，深分页会在页边界重复或漏事件**——这比慢更严重。
> - **`scripts/ci/check_ts_order_tiebreak.py` 基线因本次修复收缩**（`event/basic.rs` 2→1、`media/chunked_upload.rs` 1→0），按该脚本自身约定跑 `--update` 收紧；这是修复后的必需动作，不是放宽门禁。
>
> 测试：storage 新 db_test `test_get_room_events_by_type_applies_offset_in_sql`（先以"忽略 offset"取得行为红：返回最新页而非第二页）、sliding_sync 新 db_test `test_get_rooms_for_list_ties_break_on_room_id`（先取得红：插入序泄漏成页序）。`event::db_tests` 89 passed、`sliding_sync::db_tests` 36 passed、`event_storage_tests_migrated` 6 passed。

### PERF-09（P2）邀请黑名单 upsert 缺 `FOR UPDATE`，存在并发竞态
- **位置**：`synapse-storage/src/invite_blocklist.rs:53-76,112-135`
- **类型**：并发正确性
- **影响范围**：并发重复拉黑/解除会读到旧行并覆盖对方写入，导致计数或状态丢失。
- **修法**：`SELECT ... FOR UPDATE` 或改为单条 `INSERT ... ON CONFLICT DO UPDATE`。

> **落地状态（2026-10-22，W4 第二层已实现，提交 `0d18fede0`）**。实际写法是"事务 + 按房间行锁 + 批量 `ON CONFLICT DO NOTHING`"：
>
> - `set_invite_blocklist` / `set_invite_allowlist` 各自开事务，先 `lock_room_row(tx, room_id)` 把同房间的写者串行化；
> - `DELETE` 与 `INSERT` 在同一事务内 ⇒ 读者不会看到"先清后插"之间的空窗（那个空窗会被读成"无限制"，直接放行被拉黑者）；
> - 插入改 `INSERT ... SELECT $1, unnest($2::text[]), $3 ON CONFLICT DO NOTHING`，一次往返写入整批，重复项不再报错。
>
> 原文建议的"`ON CONFLICT` 前加 `SELECT ... FOR UPDATE`"对**整表替换语义**不够：真正要防的是"两个替换交错"，所以锁的是房间而不是单个黑名单行。

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
- **第三轮补充**：347 个 `CREATE INDEX` 的**索引名互不重复**，因此不是"重复计数"造成的差异，确实是台账少记了 5 条。

### PERF-13（P3，第三轮新增）URL 预览首跳被 DNS 解析两次
- **位置**：调用方 `synapse-web/src/routes/media/preview.rs:64`（`check_url_and_resolve`）→ service `synapse-services/src/url_preview.rs` 的 `fetch_html`（内部再次解析首跳）
- **类型**：重复外部调用
- **影响范围**：每次预览多一次 DNS 查询（含超时风险）；若两次解析结果不同（DNS 轮询 / 记录变更），
  还会出现"校验的 IP 与实际连接的 IP 不同"的 TOCTOU 窗口——SSRF 防护依赖的就是这条链路。
- **修法**：让 `check_url_and_resolve` 返回已解析的 `IpAddr` 列表，`fetch_html` 复用该结果
  （即"解析一次、钉住复用"），使校验目标与连接目标严格同一。
- **注意**：这是**正确性**修复而非纯性能优化，优先级可上调。
- **状态：已修（2026-10-22，W4 第三层）**。改法与原计划不同：**没有改 `check_url_and_resolve` 的签名**（它是 `synapse-common` 的公共工具，加返回值会把"校验"和"连接钉住"两件事绑进公共 API），改为在 `url_preview` 侧新增 `VerifiedFirstHop { url, host, ips }`，由 HTTP 层用 `check_url_and_resolve` 的输出构造后透传（`media_service::preview_url(.., Option<&VerifiedFirstHop>, ..)`）。`resolve_hop` 仅当 `current` 与 `first_hop.url` **逐字节相同**时才复用地址，否则照常解析——重定向的每一跳仍重新校验，DNS-rebinding 的钉住逻辑没有变弱。用例 `the_first_hop_uses_the_pre_verified_resolution`（该组 19 passed）。

### PERF-14（P2，第三轮新增）分片上传在内存中拼完整个文件后才校验大小上限
- **位置**：`synapse-services/src/media/chunked_upload.rs`（`load_completed_upload`）
- **证据**：
  ```rust
  let mut combined_data = Vec::new();
  for chunk in chunks { combined_data.extend_from_slice(&chunk.data); }   // 先全量拼接
  if combined_data.len() as i64 > self.max_file_size { return Err(...) }  // 后校验
  ```
  单块上限 10 MiB，但块数不限；`max_file_size` 来自 `config.server.max_upload_size`。
- **类型**：内存放大 / 资源耗尽
- **影响范围**：一次分片上传的峰值内存 ≈ **文件总大小**（而非单块大小）。若 `max_upload_size`
  配成数百 MiB 或 GiB，N 个并发分片上传可把进程打满——这是**可从外部触发**的 OOM 面。
- **修法**：① 拼接前用 `chunks.iter().map(|c| c.data.len()).sum()` 预检总量（O(块数)，不分配）；
  ② 更彻底的做法是 `Vec::with_capacity(total)` + 边拼边比，或直接流式写入对象存储不拼接；
  ③ 与 FUNC-06 的累计校验联动——在 `upload_chunk` 阶段就拦住，让这条路径根本走不到超限。
- **回归用例**：构造"单块合规、总量超限"的分片序列，断言在**拼接之前**就返回错误（用内存分配断言或 `with_capacity` 未被调用）。
- **状态：已修（2026-10-22，W4 第三层）**。取 ①②：抽出 `checked_chunk_total(&[Vec<u8>], max_file_size) -> Result<usize, ApiError>`（只做 `map(Vec::len).sum()`，不分配），`load_completed_upload` 先校验再用返回值 `Vec::with_capacity(total)` 一次预留，边拼边不再增长。**没有取 ③ 的"根本走不到"**：FUNC-06 的累计校验在 `upload_chunk` 侧，二者是纵深防御的两层，保留 ② 是有意为之。用例 `oversized_assembly_is_rejected_before_the_buffer_is_allocated`。诚实说明：修复前后对外行为都是 400，无法取得行为级红，红证是"提取纯函数后旧调用点编译失败"这一个结构红。

---

## 五、安全性

### SEC-01（P1）RBAC 总开关关闭时，`super_admin` 专属黑名单一并失效（fail-open）
- **位置**：`synapse-web/src/utils/admin_auth.rs:66-67`（`let rbac_allowed = !rbac_enabled || allowed;`）；黑名单定义 `synapse-web/src/utils/admin_auth/privileged.rs:15-60`
- **类型**：授权绕过（配置相关）
- **影响范围**：`security.admin_rbac_enabled=false` 时，任何 `is_admin=true` 的账号（含 `user_admin`、`auditor` 等）都能访问只应属于 `super_admin` 的高危端点。开关的本意是"关闭角色细分"，而不是"关闭超管专属限制"，当前实现把两者耦合了。
- **证据**：`is_role_allowed` 内部先判 `super_admin` 再判黑名单；短路后 `allowed` 完全不被参考。
- **修法**：把 `is_super_admin_only_endpoint(method, path)` 提到开关外，做成独立的无条件拒绝；开关只影响 `allowlist::role_allows`。
- **状态：已修（2026-10-22，W2）**。`admin_auth.rs` 改为「`super_admin` 短路 → 黑名单无条件拒绝 → 开关 → 角色细分」。门禁：`rbac_switch_does_not_reopen_super_admin_only_endpoints`（端点集从 `ROUTE_HANDLERS.json` × 生产黑名单自动生成，≥20 条，含 `super_admin` 正控）+ `rbac_switch_still_disables_role_splitting`，`cargo test -- admin_auth` 22 passed。

### SEC-02（P2）默认管理员口令 `Admin@123` 在脚本与文档中扩散
- **位置**：`docker/deploy/run_permission_matrix.sh:33`（硬编码）、`api-integration_test.sh:40`、`scripts/api_test/token_manager.py:195`、`scripts/api_test/config.yaml:37`、`scripts/api_test/run-load-test.sh:10`
- **类型**：凭据卫生
- **影响范围**：`scripts/api_test/register_admin.py:19-25` 已强制环境变量，不会自动进生产；但脚本中的硬编码值会被复制到部署环境。
- **修法**：脚本统一从环境变量读取，默认值仅限 `127.0.0.1` 且显式 `TEST_ONLY`。
- **状态：已修（2026-10-22，W5）**。默认值**直接删除**，改为「缺失即失败」（`${VAR:?msg}` / 显式 `exit 1`）：`docker/deploy/run_permission_matrix.sh`、`docker/deploy/api-integration_test.sh`、`scripts/test/api-integration_test.sh`、`scripts/api_test/{config.yaml,token_manager.py,run_api_tests.py}`、`scripts/test/perf/{run_tests.sh,api_matrix_core.js}`、`run-load-test.sh`。新增 CI 扫描 `scripts/ci/check_no_default_admin_credential.sh`（`scripts/**`+`docker/**` 排除 `*.md`）并接入 `ci.yml`；红态实测通过（注入 `# probe: Admin@123` → exit 1）。

### SEC-03（P2）MFA 配置只校验"非空"，占位符可当 TOTP 密钥
- **位置**：`synapse-common/src/config/validation.rs:17-21`；`synapse-web/src/utils/admin_auth.rs:269-276`（`decode_base32_secret(...).or_else(|| Some(trimmed.as_bytes().to_vec()))`）；`.env.example:60` 的 `CHANGE_ME`
- **类型**：配置校验缺失
- **影响范围**：误用示例值时 MFA 形同虚设（任意人可用固定密钥推码），且不会报错。
- **修法**：拒绝已知占位符与过短密钥，非法 base32 直接启动失败而非退化为字节串。
- **状态：已修（2026-10-22，W5）**。`synapse-common/src/config/validation.rs::validate_admin_mfa_secret`：≥16 字符 + 占位符黑名单 + `A-Z2-7` 字符集校验；`admin_auth.rs::decode_secret` 去掉"解码失败回退原始字节"。`.env.example` / `docker/deploy/.env.example` 的 `CHANGE_ME` 与 fixture `"mfa-secret"` 统一替换为 `JBSWY3DPEHPK3PXP`。门禁：`cargo test -p synapse-common --lib` 992 passed（含 4 条新用例）+ `admin_auth` 5 条。

### SEC-04（P2）未设置 `PROMETHEUS_AUTH_TOKEN` 时 `/metrics` 完全匿名
- **位置**：`src/server/mod.rs:1319-1322`
- **类型**：信息暴露
- **影响范围**：指标含房间数、用户数、请求分布、错误率，可被外部枚举业务规模与攻击面。
- **修法**：无 token 时默认仅绑定 loopback，或启动告警并要求显式 `metrics.public = true`。
- **状态：已修（2026-10-22，W5）**。`src/server/mod.rs` 引入三态 `PrometheusAuth::{Token, Public, Disabled}`：有 `PROMETHEUS_AUTH_TOKEN` → 校验 `Bearer`；无 token 且 `prometheus.public=true` → 公开；其余 → 拒绝（fail-closed，不再出现"未配置即匿名"）。`PrometheusConfig.public` 默认 `false`。部署栈在 `docker/config/homeserver.yaml` 显式开启（Prometheus 抓取无认证头，且仅绑 `127.0.0.1`）。门禁：`metrics_endpoint prometheus` 7 passed。

### SEC-05（P2）MFA 敏感操作只覆盖写方法与 3 个路径前缀，读接口可无 MFA 取明文凭据
- **位置**：`synapse-web/src/utils/admin_auth.rs:209-217`
- **类型**：MFA 覆盖缺口
- **影响范围**：`GET .../registration_tokens`、`GET .../appservices` 返回明文 token/as_token，仅凭 access token 即可读取；一旦管理 token 泄漏，MFA 不能阻断凭据二次窃取。
- **修法**：把"返回凭据类"的 GET 纳入敏感集合（按响应语义而非方法判断）。
- **状态：已修（2026-10-22，W5）**。`is_sensitive_admin_request` = 写方法一律敏感 + 既有路径前缀 + `is_credential_returning_admin_read`（`/_synapse/admin/v1/registration_tokens`）。
  > **原文更正**：实测 `AppServiceResponse` **不含** `as_token` 字段，`GET .../appservices` 并不返回明文凭据，故不纳入敏感集合。真正返回明文凭据的读接口只有 `registration_tokens`。

### SEC-06（P2）审计日志来源 IP 可经 `X-Forwarded-For` 伪造
- **位置**：`synapse-web/src/middleware/auth.rs:251-256`（以 `peer_addr=None, trusted_proxies=&[]` 调用）；`synapse-web/src/utils/ip.rs:66-67`（无条件解析 XFF）
- **类型**：审计污染
- **影响范围**：攻击者可伪造审计记录中的来源 IP，事后溯源失效；`peer_addr=None` 时本应"不可知"，却被 XFF 填充。
- **修法**：仅在 `trusted_proxies` 命中时采信 XFF；否则只用 `peer_addr`，都无则记 `unknown`。
- **状态：已修（2026-10-22，W5）**。`extract_client_ip`：无 peer ⇒ `None`（**不再回退转发头**）；peer 不在可信代理列表 ⇒ 直接用 peer；命中才解析 XFF，解析不出仍回落 peer。`auth.rs` 从 `ConnectInfo<SocketAddr>` 取 peer。另拆 `deny_only_forwarded_ip` 供"拒绝本机以外"的加严判定复用（`localhost_guard`、`admin/register`）。
  > 规格变更：原用例 `no_peer_falls_back_to_header` 改名为 `no_peer_does_not_trust_forwarded_headers` 并断言 `None`，另加 `deny_only_forwarded_ip_works_without_a_peer`。门禁：`cargo test -- utils::ip` 26 passed。

### SEC-07（P2）联邦重放缓存窗口 300s 与签名容差 24h 不匹配
- **位置**：`synapse-web/src/routes/state.rs:88-92`（`window_secs: 300`、进程内、`10_000` 容量）vs `synapse-common/src/config/federation.rs:285-287`（24h）
- **类型**：重放防护不足
- **影响范围**：300s 后缓存项被淘汰，签名仍在 24h 有效期内的事件可重放；进程内缓存也无法跨实例共享，多副本部署下窗口进一步缩小。
- **修法**：窗口对齐签名容差；缓存改为 Redis（已有缓存层），并设容量与淘汰策略。
- **状态：已修（2026-10-22，W5）**。`ReplayProtectionConfig::for_federation(enabled, signing_ts_tolerance_ms)` 让窗口**从容差推导**（`federation_window_secs` = `max(容差, 300s)`，单一真源），容量 `100_000`；`routes/state.rs` 改用它。跨实例层：`federation_auth.rs` 叠加 `CacheManager::try_acquire_lock("fed:replay:{sig_hash}")`（Redis `SET NX EX`），命中即 401；Redis 故障 fail-open + `warn!`（不谎报已防重放）。门禁：`cargo test -p synapse-common --lib` 992 passed（含 2 条窗口推导用例：24h→86400、0/-1/60s/299s→300、disabled 透传）。

### SEC-08（P2）监控栈默认口令 `admin123`
- **位置**：`docker/docker-compose.monitoring.yml:113`（`${GRAFANA_ADMIN_PASSWORD:-admin123}`）
- **类型**：默认凭据
- **影响范围**：未设环境变量时 Grafana 以弱口令暴露；若端口被映射到公网即失陷。
- **修法**：去掉默认值，缺失即启动失败。
- **状态：已修（2026-10-22，W5）**。`docker/docker-compose.monitoring.yml` 改为 `${GRAFANA_ADMIN_PASSWORD:?GRAFANA_ADMIN_PASSWORD must be set (no default credentials)}`；`.env.monitoring.example` 置空并注释；`MONITORING_GUIDE.md`、`README_MONITORING.md`、`monitoring/DEPLOYMENT.md`、`scripts/deploy-monitoring.sh` 同步。门禁：缺失/空值 → `docker compose config` exit 1；有值 → exit 0。

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
- **状态：已修（2026-10-22，W2）**。`synapse-federation/src/signing.rs` 新增 4 条 `test01_*` 用例（合法/时钟漂移/未知 key id/篡改；后两条分别覆盖密钥轮换与信任根失败语义）。门禁：`cargo test -p synapse-federation --lib -- test01` **4 passed / 0 failed**；`-- signing` 37 passed 无回归。未用 `assert!(false)` 占位跑红（会污染既有可运行用例），红态由"crypto 层不做新鲜度检查"这一断言承担。

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
> **第三轮更新（2026-10-22）**：实测数字已从 277 漂移到 **283**，三处文档需同步。
- **位置**：`docs/synapse-rust-vs-synapse-comparison.md:413`（238 个 `.rs`）vs 实测 **283**（unit **128** + integration **147** + e2e **3** + performance **5**）；`.trae/rules/project_rules.md` §9.1（115/130/3/5）vs 实测 128/147/3/5（unit、integration 两项过时）
- **类型**：度量漂移
- **影响范围**：无法用文档判断覆盖趋势。
- **修法**：接入已有的度量脚本，文档只引用。建议脚本输出固定为 `unit/integration/e2e/perf = N/N/N/N` 单行，文档直接粘贴该行。

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
- **状态：已简化（2026-10-23），但与原建议不同 —— 原建议会丢掉 `0` 校验**。
  `0` 在 serde 眼里是**合法整数**，而唯一消费点（`wiring/admin.rs:260`）拿它当 AS 调度器每 tick 的
  派发上限 ⇒ `0` 意味着"应用服务永不运行"且不报任何错。所以"去掉自定义 deserializer"不是等价简化。
  实测（`test_invalid_inputs_are_rejected` 的 6 例表）证明其余非法输入 `true` / 负数 / 浮点 /
  字符串 / `null` / 数组 **serde 自己就拒**，因此自定义反序列化的真实价值只有两条：`> 0`、
  以及 `usize` 装得下。据此重写：去掉 `serde_json::Value` 中间层与"拒绝布尔"那条分支
  （它只多给一句友好文案，不是唯一拦截点），改为直接收 `u64` 再 `try_from`（顺带把 32 位目标的
  越界语义说清）；测试 7 条 → 5 条，三条单值负控合成一张 6 例表并新增数组一例。
  行数 111 → 111（文档等量替换），但关键路径少一次 `Value` 物化与一个多余分支。
  红证：删掉 `> 0` 校验 ⇒ `test_zero_is_rejected` FAILED；还原 ⇒ 5 passed。

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
| `#[ignore]` 真实属性 | **16 条 / 7 文件**（`tests/e2e/user_flow_tests.rs` 7、`tests/performance/*` 4、`tests/unit/pagination_gate_tests.rs` 2、`synapse-web/src/utils/admin_auth/admin_rbac_tests.rs` 1、`synapse-e2ee/src/{crypto/aes.rs,ssss/service.rs}` 各 1），全部带 `#[ignore = "…"]` 理由，无"无理由 ignore"、无"引用未建 issue"。**第三轮的"11 条"只数了 `tests/` 下的两个目录，漏了 `src/` 与 `tests/unit/`，见 §0.3** |
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

---

## 十、优化方案（分阶段、可执行）

### 10.0 排序原则

不是按"P1 先做、P2 后做"排，而是按**三个正交维度**排，因为按级别排会连续三个批次都在改缓存、无法并行评审：

| 维度 | 判据 | 为什么重要 |
|---|---|---|
| **风险** | 不修会不会造成用户可见的错误数据/越权 | 决定"能不能等" |
| **爆炸半径** | 改动是否只是机械替换，还是触及 API/DB/契约 | 决定"要几个人评审" |
| **可验证性** | 能否写出一条**先红后绿**的测试 | 决定"能不能证明修好了" |

**铁律：每个批次必须先提交"能失败的验证"，再提交修复。** 拿不出可失败验证的条目，一律降级为"登记项"而不是"任务项"——否则修完无法证明修好了，下一轮审查还会把它重新列出来（本文件前三轮已经出现过这种循环：CODE-11 的"无收敛策略"其实就是没先确认棘轮是否已存在）。

### 10.1 批次总表

| 批次 | 主题 | 条目 | 改动性质 | 预估评审面 | 依赖 | 状态 |
|---|---|---|---|---|---|---|
| **W0** | P1 一致性收口 | CODE-02、CODE-03 | 根因修复（cache 墓碑）+ 2 个失效 helper + CI 扫描（19+6 处） | 中（但模式统一，见 §10.2） | 无 | ✅ 已提交 `8b8505ff5` |
| **W1** | P1 配额与事务 | FUNC-06 → FUNC-03、PERF-02 | 契约微调 + 事务包裹 | 中 | 无 | ✅ 已提交 `cc364bd6f`（见 §10.3） |
| **W2** | P1 授权与信任根 | SEC-01、CODE-05、TEST-01 | 单点逻辑 + cfg 门禁 + 补测试 | 小 | 无 | ✅ 已提交 `85180061c`（见 §10.4） |
| **W3** | P1 结构决策 | ARCH-01 | 方案 A「打通」：写路径改走规范事件，影子表降级为索引 | **大** | 需先定方案 | ✅ 已提交 `3789583ef`（见 §10.5） |
| **W4** | P2 资源与性能 | PERF-14、PERF-13、PERF-03/04/05、PERF-07/08/09、PERF-06、CODE-04、CODE-07 | 索引/查询/分批 | 中 | W1（FUNC-06 累计校验） | ✅ 三层全部落地：第一层 `2358c0f5a`、第二层 `0d18fede0`、第三层 `3cf5c006a`（见 §10.6） |
| **W5** | P2 安全一致性 | SEC-02…SEC-08 | 配置校验 + 默认值 | 小 | 无 | ✅ 已提交 `ca6980c51`（见 §10.7） |
| **W6** | P3 冗余与文档 | 全部 P3 + §9.12 五批 | 删除/合壳/文档 | 小但量大 | W0（避免删到正在改的文件） | 🟡 批次 1 已提交 `e4ac05ce0`、批次 2 已提交 `b51aafb00`、批次 3 已提交 `f7bf4eec0`（见 §10.8），批次 4…5 待开始 |

**建议并行度**：W0 / W2 / W5 三者互不重叠，可并行；W1 与 W4 串行；W3 单独走设计评审；W6 最后收尾（它会删文件，先做会和前面所有分支冲突）。

---

### 10.2 W0：P1 一致性收口（CODE-02 + CODE-03）

> **落地状态（2026-10-22，本批次已实现，提交 `8b8505ff5`）**。本节原计划与实际实现有 3 处偏差，以实际为准：
> ① 不新建 `cache_invalidation.rs`（`room_state` 的 key/失效契约本就声明在 `synapse-services/src/room_state_cache.rs` 文件头，在那里加 helper 才符合"同一 key 只有一处实现"）；
> ② 墓碑从"全局静态 + 短 TTL"改为 `CacheManager` 的**实例字段、无 TTL**（短 TTL 只是把旧值复活推迟一个 TTL，不是修复）；
> ③ 签名是 `&CacheManager`（仓里没有 `Cache` 这个类型），且 `CacheManager::delete` 返回 `()`，真实缺陷不在返回值丢失，而在**L1 已删、L2 DEL 失败、下一次读从 L2 回填把旧值复活**。

**根因（为什么不是机械替换就能修好）**

`CacheManager::delete` 内部对 Redis 失败已经 `warn!` 过，所以 19 处 `let _ =` 丢掉的并不是错误——真正的故障链是：

```
delete() → local.remove(key) 成功 → redis.delete(key) 失败（Redis 抖动/熔断）
        → 下一次 get() → L1 miss → L2 命中旧值 → backfill_l1(旧值)
        → L1 重新持有旧值，直到 L2 的 TTL（最长 3600s）到期
```

因此修复必须落在 `synapse-cache` 的读路径上，services 层换 helper 只解决"同一个 key 不要在 15 个地方各自拼字符串"。

**改动 1：`synapse-cache/src/manager.rs` 墓碑机制**

- 新增字段 `tombstones: Arc<parking_lot::RwLock<HashSet<String>>>`（5 个构造函数全部初始化）。
- `record_delete_outcome(&self, key, outcome: Result<(), CacheError>)`：成功 → 摘墓碑 + `note_cache_operation(..., "success")`；失败 → `warn!` + 打墓碑。
- `set` / `set_raw` / `set_checked` 在 L2 写成功后摘墓碑（这是墓碑唯一的正常摘除条件）。
- `get` / `get_checked` / `get_raw_shared` 的 L2 分支加 `!self.is_tombstoned(key)`；`get_batch` 对墓碑 key 跳过 MGET。
- **`get_raw` 不加墓碑判断**：墓碑只拦 L2 读，不拦 L1 读。否则 Redis 长时间故障时，新写入在 L1 里也读不到，等于用可用性换一致性。
- `delete_batch` / `delete_with_invalidation` 同样接入（部分失败 → 批量打墓碑）。

**改动 2：key 的单一失效点（不新建文件）**

| key | 唯一实现 | 替换处 |
|---|---|---|
| `room_state:{room_id}` | `room_state_cache.rs::invalidate_room_state(cache, room_id)` | 15 处（create/actions/federation/moderation/events + sliding_sync 测试） |
| `account_data:{user_id}` | `account_data_service.rs::{account_data_cache_key, invalidate_account_data}` | 3 处（account_data_service ×2、friend_room_service ×1）；`sync_service/data_fetch.rs` 的读 key 也切到 `account_data_cache_key`，读写不再各拼一份 |
| `user:profile:{user_id}` | 仅 `synapse-web/src/federation/edu.rs:785` 一处 | 直接去掉 `let _ =`，不建 helper（唯一删除点，抽 helper 是过度抽象） |

**改动 3：CODE-03 `synapse-storage/src/device/mod.rs` 6 处**

与 D-37 的 `:566` 对齐，但按"事务内 / 提交后"分流：

- 事务内（`:494`、`:673`）→ `.await?` 向上传播，事务回滚，不留半删状态；
- 事务已提交（`:634`、`:730`、`:770`、`:794`）→ 行已经删掉，回滚无意义，改为 `warn!` 记录 `user_id`/`device_id`，不把已成功的删除变成用户可见错误。

**门禁（均已实测"能红"）**

| 门禁 | 形态 | 红态证据 |
|---|---|---|
| `failed_l2_delete_does_not_resurrect_the_stale_value` | 用 `record_delete_outcome(key, Err(...))` 精确构造"L2 有值但 DEL 失败"，断言随后 `get` 为 `None`、`set` 新值后能读到新值 | 移除墓碑判断 → 读回 `stale_value` |
| `tombstone_does_not_hide_fresh_local_writes_during_redis_outage` | 指向坏 Redis（`127.0.0.1:1`）：`set v1` → `delete` → `get` 必须 `None` → `set v2` → `get` 必须 `Some("v2")` | 把墓碑判断加进 `get_raw` → 第二条断言失败（可用性回归） |
| `scripts/ci/check_no_silent_cache_delete.sh`（接入 `ci.yml`） | 扫 `git ls-files '*.rs'` 中的 `let _ = …cache.delete(_batch)?(`，剔除纯注释行，命中即 exit 1 | 2026-10-22 实测：把 `edu.rs:785` 改回 `let _ =` → `FAIL` + exit 1；还原 → `OK` |
| 回归 | `cargo test -p synapse-cache`（95 passed）；`cargo check -p synapse-services/-storage/-web --all-targets`（+`--features test-utils`）均 exit 0；`cargo clippy --workspace --all-targets --features test-utils -- -D warnings` exit 0；成员集成回归（`--test integration … membership`）**85 passed / 0 failed**（join/leave/ban/unban 用例全绿，未改断言） | — |

> 集成回归需 `TEST_DB_ISOLATION=database`（从模板库克隆；`bash scripts/test_db_template.sh ensure` 先备好模板）。直连空库 `synapse_test` 会以 `42P01 relation "users" does not exist` 全线失败——那是环境问题，不是本批次回归。
>
> 另注：`scripts/ci/check_file_size_ratchet.py` **在本批次之前就已失配**（HEAD 上 24 个文件的 baseline 落后于实际行数，如 `push/mod.rs` 1553→1845）。本批次只上调了自己触达的 5 个条目（`device/mod.rs`、`manager.rs`、`moderation.rs`、`cache/src/tests.rs`、`account_data_service.rs`），未 `--update`，以免把 24 条无关增长一并"批准"。该既有债务需单独立项处理。

**回滚**：墓碑是纯内存结构，无持久化副作用；三处改动（cache 墓碑、helper 替换、device 吞错）互相独立，可分别 revert。回滚后 CI 扫描会立刻变红，这正是期望的失败信号。

**验收标准**：扫描为 0；两条墓碑用例绿（Redis 不可用时第一条 skip，第二条用坏后端恒可跑）；成员 join/leave/ban 的既有集成测试全绿（不得修改断言）。

---

### 10.3 W1：P1 配额与事务（FUNC-06 → FUNC-03 → PERF-02）

顺序很重要：**先堵住"多传"（FUNC-06），再补"记账"（FUNC-03）**。反过来做，中间态会出现"记账了但没拦住"的更糟组合。

> **落地状态（2026-10-22，本批次已实现，提交 `cc364bd6f`）**。本节原计划与实际实现有 5 处偏差，以实际为准：
> ① PERF-02 **没有**在 `register_user` 外层包事务（业务层拿不到 `PgPool`），而是把事务下沉到存储层，以 `UserStore::create_registration(NewRegistration)` 作为唯一边界方法——业务层只调一次，五次写入与整体回滚成为该方法的实现细节，`AuthService` 与测试夹具只需各补一个实现；
> ② 访问/刷新令牌改为**事务外预生成**（`build_access_token` / `build_refresh_token` 返回纯值），事务内只做 INSERT，避免把 CSPRNG 与哈希放进持锁路径；
> ③ FUNC-03 用「新增带记账语义的变体」而非给既有函数加枚举参数：`upload_media_with_id_accounted(..., exclude_upload_id: Option<&str>)`，`exclude_upload_id` 让 `complete` 排除该会话自身的预占（否则"预占 + 实占"重复计数）；
> ④ FUNC-06 的 413 在**两层**都做：服务层按 `max_file_size` 判、web 层按 `server.max_upload_size` 早拒绝（同一 errcode），避免超大声明值先把 body 读完才被拒；
> ⑤ `progress.total_size` 为空的历史 pending 记录保留回退（以 `max_file_size` 作硬上界），加严不会让在途会话卡死。
>
> **红/绿证据（`register_is_atomic`）**
>
> - 绿（正式实现）：`Creating user in transaction` → `ERROR … violates check constraint "register_is_atomic_no_insert"` → `1 passed`。
> - 红（临时把用户行改成独立提交，模拟修复前）：`assertion left == right failed: 注册失败后 users 不得残留行 left: 1 right: 0` → `FAILED`；还原注入后重跑恢复 `1 passed`。
> - 其余三条：`cargo test -p synapse-services --lib --features … chunked_upload` → `6 passed`（含 `chunked_upload_requires_total_size`、`chunked_upload_rejects_lying_total_size`、`chunked_upload_counts_toward_quota`）。
>
> **契约**：`ROUTE_CONTRACT.md` 附录 E.17（生成器保留的人工尾段）标注 `total_size` 必填与 400→413；提交后 `check_route_contract.sh` 复核通过（生成物与源码路由面一致）。

**FUNC-06 改动**

- `start_chunked_upload`：`total_size` 改为必填（缺失 → 400 `M_MISSING_PARAM`）；`total_size > max_upload_size` → 413。
- 预占配额（reservation）：`start` 时按 `total_size` 记 `pending_bytes`，`complete` 时转成 `used_bytes`，`abort`/过期时释放。
- `upload_chunk`：`uploaded_size + chunk.len() > total_size` → 413 并终止 upload。

**FUNC-03 改动**

- 把 `record_upload_usage` 下沉进 `upload_media_with_id`（或用 `UploadAccounting::{Record, Skip}` 参数显式区分），使"新增上传路径忘记记账"在类型层面不可能。

**PERF-02 改动**

- `register_user` 外层包 `sqlx::Transaction`：`create_user` + `update_displayname` + `create_device` + `access_token` + `refresh_token` 一个事务；失败整体回滚。
- 注意：`update_displayname` 当前"失败仅 warn"，进事务后必须改为传播错误，否则事务内 warn 无意义。

**门禁**

| 门禁 | 红的方式 |
|---|---|
| `chunked_upload_rejects_lying_total_size`（声明 1 KiB 传 2 KiB，第 2 块 413） | 去掉累计校验即失败 |
| `chunked_upload_requires_total_size` | 允许 `None` 即失败 |
| `chunked_upload_counts_toward_quota`（上传后 `quota_used` 增加） | 保持现状即失败 |
| `register_is_atomic`（在 `refresh_token` 插入处注入失败，断言 `users` 表无残留行） | 去掉事务即失败 |

**回滚**：FUNC-06 是请求校验加严，属**契约变更**——需在 `ROUTE_CONTRACT.md` 标注 `total_size` 必填，并在发布说明中提示客户端。其余两项无契约影响。

**验收标准**：4 条用例先红后绿；`ROUTE_CONTRACT.md` 与实际校验一致（用 `api_route_ledger_tests` 兜底）。

---

### 10.4 W2：P1 授权与信任根（SEC-01 / CODE-05 / TEST-01）

| 条目 | 改动 | 门禁（能红） |
|---|---|---|
| SEC-01 | 把 `is_super_admin_only_endpoint` 提到 `admin_rbac_enabled` 判断之外，成为无条件拒绝 | 表驱动测试：`admin_rbac_enabled=false` × 全部超管专属端点 ⇒ 非超管一律 403；删掉提取即红 |
| CODE-05 | `user_store_fake` 加 `#[cfg(any(test, feature = "test-utils"))]`；`pool()` 改为 `Err` | `cargo build --workspace`（默认 feature）后 `nm`/`grep` 确认符号不存在；`--features test-utils` 时存在 |
| TEST-01 | 为联邦签名补 4 类用例：合法签名、时钟漂移超容差、未知 key id、签名体篡改 | 用例先以 `assert!(false)` 占位跑红，实现后转绿；覆盖率工具确认 `signing.rs` 分支覆盖上升 |

**回滚**：均为局部改动；SEC-01 若引发误拒，可单独 revert 该提交（不牵连其他两条）。

> **落地状态（2026-10-22，本批次已实现，提交 `85180061c`）**。本节原计划与实际实现有 3 处偏差，以实际为准：
> ① SEC-01 采用「**前置无条件拒绝**」而非「把黑名单提到开关外再判」——`super_admin` 短路之后**立刻**判黑名单，`admin_rbac_enabled` 只在最后决定是否做角色细分。两者语义等价，但前置写法让"黑名单不受任何开关影响"在控制流上不可绕过（不依赖读者确认后续分支没有提前 return）；
> ② CODE-05 用**整模块 `#[cfg]`**（`synapse-storage/src/lib.rs` 的 `mod user_store_fake` + `pub use` 同时门禁）而非符号级 `#[cfg]`，否则模块内 `unimplemented!` 的辅助类型仍会被编译；
> ③ TEST-01 未用 `assert!(false)` 占位跑红——`signing.rs` 原有用例可运行，占位断言只会污染文件；改为"先写完整用例、跑出 4 条对既有实现的真实判定"，其中 `test01_clock_drift_outside_tolerance_is_rejected` 的红态由"证明 crypto 层不做新鲜度检查"这一断言承担（该断言在把容差检查移出调用方后会立刻失败）。

**改动 1：SEC-01 `synapse-web/src/utils/admin_auth.rs`**

```rust
fn is_admin_request_allowed(role: &str, method: &Method, path: &str, rbac_enabled: bool) -> bool {
    if role == "super_admin" { return true; }
    // 黑名单**先于**角色匹配与开关：所有非超管角色（含 auditor / security_admin）都受它约束。
    if is_super_admin_only_endpoint(method, path) { return false; }
    // 开关只关闭「角色细分」，不放宽上面的黑名单。
    if !rbac_enabled { return true; }
    allowlist::role_allows(role, method, path)
}
```

**改动 2：CODE-05 `synapse-storage/src/user_store_fake.rs`**

- `FakeUserStore` 持有 `pool: Arc<PgPool>`，未实现的方法统一 `Err(StorageError::…("not implemented in FakeUserStore"))`，不再 `unimplemented!()`；
- 模块与再导出同时挂在 `#[cfg(any(test, feature = "test-utils"))]` 下，默认 feature 构建进不去符号表。

**改动 3：TEST-01 `synapse-federation/src/signing.rs`（+4 用例，共 1383 行）**

| 用例 | 断言要点 |
|---|---|
| `test01_valid_signature_over_canonical_edge_content` | room v11 的 `m.room.create`，content 含 unicode / astral 平面字符 / 嵌套数组 / 转义引号反斜杠换行；两种键序的 canonical JSON 等价、签名等价；签名材料确实含 unicode 与嵌套数组 |
| `test01_clock_drift_outside_tolerance_is_rejected` | 过去、未来各 1 条超容差 → 拒；窗口内正控 → 过；并断言 **crypto 层本身不做新鲜度检查**（新鲜度由调用方负责） |
| `test01_unknown_key_id_is_rejected` | 错误信息必须含 `No verify key for ed25519:2`；用已发布 key id 做正控 |
| `test01_tampered_body_fails_hash_and_signature` | 篡改 `content.name` → 同时报 `content hash mismatch` 与 `Signature verification failed` |

**门禁（实测）**

| 门禁 | 结果 |
|---|---|
| `cargo test -p synapse-web --lib -- admin_auth` | **22 passed / 0 failed / 1 ignored**（含 SEC-01 的 `rbac_switch_does_not_reopen_super_admin_only_endpoints`：端点集从 `ROUTE_HANDLERS.json` × 生产黑名单**自动取**，≥20 条，含 `super_admin` 正控；另有 `rbac_switch_still_disables_role_splitting`） |
| `cargo test -p synapse-federation --lib -- test01` | **4 passed / 0 failed** |
| `cargo clippy -p synapse-web -p synapse-storage -p synapse-federation … --all-targets --features test-utils --locked -- -D warnings` | exit 0，0 warning |

> 注：`admin_role_allowed_management_endpoints` 在 HEAD 上本就自相矛盾（断言普通 admin 可 `POST /shutdown_room`、`DELETE /rooms/{room_id}`，但 `privileged.rs:128` 将其列为超管专属、golden 矩阵亦为 `deny`）。本批次按「生产逻辑 + golden 矩阵」为准把这两条断言反转为 `assert!(!…)` 并加注释，**未改动任何生产代码**；这是修正过时断言，不是为通过测试而放宽规格。

---

### 10.5 W3：ARCH-01（Thread 影子表）—— 需要决策，不要直接动手

> **落地状态（2026-10-22，已实现，提交 `3789583ef`）**。选定 **方案 A「打通」**（产品侧确认有客户端在用非规范 API，B「下架」会直接破客户端）。实际实现与本节的偏差，以实际为准：
> ① **未做存量事件回填**。计划里的"事件回填迁移"不是一条 `INSERT … SELECT` 能完成的：影子行的 `event_id` 是客户端随手给的，`events` 里从来没有过对应事件，要补就得重新走写入口定稿 id 并参与联邦签名与摘要链，属于独立迁移工程。本批只止住**新增长**，存量行原样保留（它们本来就是旧实现的产物，回填是另一件事）。
> ② **门禁从"行数不增长"改为"每写一条 thread 必然产生一条规范事件"**：`test_thread_reply_writes_canonical_m_thread_event` 直接查 `thread_replies.event_id` 回指的 `events.content.m.relates_to`。
> ③ **建线程的根事件校验是新增门禁的必然结果**：既然影子表只能索引真实事件，`root_event_id` 不存在就必须 404（`test_create_thread_rejects_unknown_root_event`），否则仍能在影子表里造出指向空气的线程行。
> ④ **回复请求体删掉了 `event_id` / `root_event_id`**（契约变更，见 `ROUTE_CONTRACT.md` E.18）。保留字段但不用，等于留下"收了不认"的兼容 shim。
> ⑤ **未处理**：`thread_relations` 仍只写不读（与 `event_relations` 重复）；`thread_roots.sender` 记的是 API 调用者而非根事件真实 sender。
>
> 红证（同一用例在修前失败）：
> - `test_thread_reply_writes_canonical_m_thread_event` → `thread_replies.event_id = $canonical_reply_… 在 events 表里不存在`；
> - `test_create_thread_rejects_unknown_root_event` → `left: 200 right: 404`。
>
> 绿证：`--test integration api_search_thread_tests` 12 passed；`api_placeholder_contract_p0_tests` 34 passed；`-p synapse-services --lib thread_service` 24 passed。

这是全清单里唯一"写了等于没写"的结构性问题，也是最容易被误修的一条：直接删表会丢数据，直接暴露会破坏 `/sync` 语义。

**先做决策，三选一**：

| 方案 | 做法 | 代价 | 适用前提 |
|---|---|---|---|
| A. 打通 | 把 `thread_roots`/`thread_replies` 的写入改为规范事件（`m.thread` rel），影子表降级为索引 | 需要事件回填迁移 + 联邦兼容测试 | 有真实客户端在用非规范 API |
| B. 下架 | 非规范 API 返回 404 并登记为"有意不实现"，影子表只读保留 90 天 | 可能破坏现有客户端 | 确认无外部依赖 |
| C. 冻结 | 停止写入，保留读取，文档标注 deprecated | 数据继续陈旧 | 需要时间观察 |

**决策依据（必须先采集，不能拍脑袋）**：`SELECT count(*) FROM thread_roots`、`MIN/MAX(created_ts)`，以及访问日志中该 API 的调用量。采集脚本输出后再进评审。

**门禁**：无论选哪个方案，都必须有"影子表行数不再增长"的断言（A 方案则是"每写一条 thread 必然产生一条规范事件"）。

---

### 10.6 W4：P2 资源与性能（按"索引 → 查询 → 分批"三层推进）

**第一层：索引（零代码风险，先做）**

| 条目 | 改动 | 验证 |
|---|---|---|
| PERF-03 | 房间名搜索：给 `LOWER(name)` 建表达式索引，或改写查询用 `ILIKE` 前缀 | `EXPLAIN ANALYZE` 前后对比，必须从 Seq Scan 变 Index Scan |
| PERF-04 | `content::text LIKE` → `content @> jsonb_build_object(...)` 或 `jsonb_path_query` | 同上，必须命中 `jsonb_path_ops` |
| PERF-05 | `LIKE '%:%'` 前导通配 → 改用 `position(':' in x) > 0` 或 `right(x, n)` | 必须消除 Seq Scan |

> 索引类改动的验收**只能看 `EXPLAIN`**，不能看"测试通过"——测试通过不代表走了索引。建议加一个 `scripts/quality/explain_check.sh`，把三条关键查询的计划写进快照。

> **落地状态（2026-10-22，第一层已实现，提交 `2358c0f5a`）**。逐条偏差见上文 PERF-05 之后的落地引用。三点计划层面的修正：
>
> - 计划写"给 `LOWER(name)` 建表达式索引"，已否决：`CREATE INDEX IF NOT EXISTS` 对同名索引不替换定义，真实库上会被静默跳过；改为"查询侧去掉 `LOWER()`"（service 层已统一小写 ⇒ 语义等价）。顺带发现 `topic` 这一列**一直没索引**。
> - 计划写 PERF-04 改 `content @> ...` 或 `tsvector`，两者都不满足"任意子串匹配"的语义；改为 `content::text ILIKE` + 表达式 trgm 索引。
> - 门禁形态从"计划快照"改为"断言索引名/谓词形态"：逐字快照对 PG 小版本敏感，会把一次正常升级变成一次无意义的重录。另外补了 4 条**源码形态断言**——只断言计划的话，把代码改回 `LOWER(...)` 时索引仍在、计划断言依然绿。
>
> 已接入 `schema-health-check.yml`（该车道本就把基线灌进一个专用库，`EXPLAIN_CHECK_EXISTING_DB=1` 直接复用，不额外建库）。

**第二层：查询形态**

- PERF-07（3 处无 `LIMIT`）：加硬上限，超限返回 `M_TOO_LARGE` 而非截断（截断会静默丢数据）。
- PERF-08（5 处 OFFSET 深分页）：改 keyset 分页（`WHERE (ts, id) < ($1, $2)`）。**这是契约变更**：响应里的 `next_batch` 语义要同步。
- PERF-09（邀请黑名单 upsert 竞态）：`INSERT … ON CONFLICT` 前加 `SELECT … FOR UPDATE`，或直接用 `ON CONFLICT DO NOTHING` + 行数判断。

> **落地状态（2026-10-22，第二层已实现，提交 `0d18fede0`）**。三条都做完，但**没有一条按原计划照抄**：
>
> - **PERF-07**：3 处里只有 1 处真需要加 `LIMIT`（`load_chunk_data`），另 2 处是死代码/已分页路径。且**不加 `M_TOO_LARGE`**——超限发生在写侧就拒绝（`total_chunks + 1` 预检），读侧的上限是纵深防御，不是对外错误码。
> - **PERF-08**：5 处里只有 `handlers/room/events.rs` 真退化，但它**不能改 keyset**（`from` 在 doc comment 与响应里就是 plain offset，换语义即破契约），改为把窗口下推到 SQL。`sliding_sync` 的 `range` 是 MSC3575 位置窗口，保留 `OFFSET` 但补 `room_id` 决胜键（`bump_stamp` 不唯一，页边界会重/漏）。另外 3 处是 admin wire 分页参数，排序已全序，不动。
> - **PERF-09**：`SELECT … FOR UPDATE` 对"整表替换"语义不够（锁一行不阻止并发方删掉别的行），实际是**事务 + `lock_room_row(tx, room_id)` + 批量 `unnest` 插入 + `ON CONFLICT DO NOTHING`**。
>
> 逐点定性表与偏差细节见上文 PERF-07/08/09 各自的落地引用。门禁侧补了一条：修复会让 `check_ts_order_tiebreak.py` 的逐文件计数**收缩**，按脚本约定必须 `--update` 收紧（当前 67 处 / 28 文件），否则门禁红。

**第三层：分批与资源**

- PERF-06（10 个 prune 无分批）：统一 `DELETE … WHERE ctid IN (SELECT ctid … LIMIT $batch)` 循环，每批 sleep 可配。
- PERF-14：拼接前先 `sum(len)` 预检；与 FUNC-06 的累计校验联动后，此路径应不可达（保留预检作为纵深防御）。
- PERF-13：`check_url_and_resolve` 返回 `Vec<IpAddr>` 并由 `fetch_html` 复用，消除双解析 + TOCTOU。
- CODE-04：`unwrap_or_default()` → `map_err` 显式错误。
- CODE-07：客户端格式错误 → 400 而非 500。

> **落地状态（2026-10-22，第三层已实现，提交 `3cf5c006a`）**。五条都做完，四条改法与计划不同（逐条偏差见上文各自的落地引用）：
>
> - **PERF-06**：分批键用 `ctid` 而非 `id`——`presence`/`one_time_keys` 等表没有可作唯一键的 `id`，`ctid` 对整表成立。**未实现"每批 sleep 可配"**：限流由批大小（1000）承担，后台任务先做完再睡只会拉长积压。返回值仍是跨批总数，调用方日志语义不变。
> - **PERF-14**：只取 ①②（`checked_chunk_total` 预检 + `with_capacity` 预留），**不取 ③**——FUNC-06 的累计校验在写侧，读侧预检是独立的第二层纵深防御，二者不是替代关系。诚实说明：修复前后对外都是 400，拿不到行为级红，红证是结构性的（抽出纯函数后旧调用点编译失败）。
> - **PERF-13**：**没改 `check_url_and_resolve` 的签名**——它是 `synapse-common` 的公共工具，把"解析"和"连接钉住"绑进公共 API 会把两件事耦合死；改为 `url_preview` 侧新增 `VerifiedFirstHop { url, host, ips }` 由 HTTP 层构造透传。复用条件是首跳 URL 逐字节相同，重定向的每一跳照常重新解析校验。
> - **CODE-04**：`unwrap_or_default()` → `parse_stored_room_config` 显式报错，并补 `release_claim_after_failure`——认领发生在解析**之前**，不释放会把邀请永久钉在 claimed 状态。
> - **CODE-07**：`direct_map_from_account_data` 三态（对象 / 非对象 / 从未设置），顺手把 `friend_room_service` 同类的 `Internal` 改为 `InvalidInput`。
>
> 门禁侧三点交代：
>
> - **`EXPLAIN` 快照没有新增**。"外加 EXPLAIN 快照"针对的是索引层，第一层已接入 `schema-health-check.yml`（`EXPLAIN_CHECK_EXISTING_DB=1` 复用该车道的基线库）；第三层全是分批与错误码，没有新增查询形态。现有 `scripts/ci/pagination_perf_gate.sh` 只覆盖 `/messages` keyset 分页，本层不涉及——不为凑门禁造快照。
> - **"单批行数 ≤ batch_size"的断言已落地**：`prune_batches_never_ask_for_more_than_the_batch_size` 直接断言每批上限（不依赖真库），`prune_batches_stop_on_an_empty_backlog` 锁终止条件，`prune_drains_a_backlog_larger_than_one_batch` 用 `PRUNE_BATCH_SIZE + 1` 行证明跨批推进。`test(prune)` 过滤 12 条全绿。
> - **全量门禁实测**（`E2EE_INTEROP=1 cargo nextest run --workspace --all-features --no-fail-fast`）：**10736 tests run: 10720 passed, 16 failed, 16 skipped**。16 条逐条甄别如下——本批次自身引入的两条红已在本批次内修掉（基线指纹常量、我在真库用例里自建 `CREATE TABLE`），其余均可在纯 HEAD `e5153b376` 工作树或环境层面复现：
>
> | 失败用例 | 判定 |
> |---|---|
> | `api_beacon_location_tests::test_beacon_location_applies_room_backpressure_token_bucket` | 抖动（单跑通过） |
> | `api_key_backup_route_table_tests::key_backup_routes_are_wired_under_v1_and_r0` | HEAD 既有（纯 HEAD 工作树复现） |
> | `api_placeholder_contract_p1p2_tests::test_power_levels_contract_rejects_non_members` | HEAD 既有 |
> | `api_relations_authorization_tests::relations_event_type_route_filters_over_http` | HEAD 既有 |
> | `api_route_ledger_tests::declared_route_manifest_full_snapshot_matches_{default,worker_enabled}_state`（2 条） | HEAD 既有；实测快照漂移 928 → 956 **全部是 `saml`/`cas` 命名空间路由**，与 W4 无关 |
> | `sqlx_cache_tooling_guard_tests::converge_{drops_the_frozen_list_and_never_re_consults_the_reference, rejects_a_vacuous_post_check_when_the_reference_collapses_mid_apply}`（2 条） | HEAD 既有；沙箱桩脚本与测试期望脱节（`d57_probe` 是桩的 seed 数据，不是真表） |
> | `sqlx_ratio_gate_tests`（4 条）+ `sqlx_dynamic_literal_guard_tests::no_new_production_literal_dynamic_sql` | HEAD 既有计数漂移（纯 HEAD 工作树同样红） |
> | `test_ddl_guard_tests::no_unallowlisted_self_built_schema_in_tests_dir` | HEAD 既有：指向 `tests/integration/registration_service_tests_migrated.rs::register_is_atomic:189`，本批次未触碰该文件 |
> | `monitoring::db_tests::database_monitor_round_trip_on_the_migration_template` | 环境：`pg_stat_database.deadlocks` 是库级累计值，并行历史累积（实测 5） |
> | `worker::tests::test_worker_type_owned_route_prefixes` | HEAD 既有：`a796b7e67`（W-F）改了 `worker/models.rs` 的前缀表未同步该用例 |
>
> 另：`.sqlx` 净增 0（删 11 增 11，条目仍 1327），`scripts/ci/check_sqlx_cache_fresh.sh --compile` 通过。

**门禁**：`cargo nextest run --workspace`；外加 `EXPLAIN` 快照（新增）；prune 用例需断言"单批行数 ≤ batch_size"。

---

### 10.7 W5：P2 安全一致性（SEC-02…SEC-08）

| 条目 | 改动 | 门禁 |
|---|---|---|
| SEC-02 | 5 处硬编码口令改环境变量 + `TEST_ONLY` 前缀 | CI 扫描 `Admin@123` 在 `scripts/`、`docker/` 下为 0 |
| SEC-03 | MFA secret 校验：非空 + 长度 ≥16 + Base32 可解码 + 拒绝 `changeme`/`xxx` 类占位符 | 表驱动：`"changeme"` 必须启动失败 |
| SEC-04 | 未设 token 时 `/metrics` 默认拒绝（与"空值即关闭"相反），仅当显式 `PROMETHEUS_AUTH_DISABLED=true` 才匿名 | 不设 env 时 `GET /metrics` 401 |
| SEC-05 | MFA 敏感操作判定改为"读接口也覆盖"（按资源而非方法），至少覆盖凭据导出/密钥读取类路径 | 无 MFA 读凭据端点必须 403 |
| SEC-06 | 审计 IP 默认取 `ConnectInfo` 的 peer addr；仅当配置了可信代理列表才解析 `X-Forwarded-For` | 伪造 XFF 不改变审计记录 |
| SEC-07 | 重放缓存窗口 ≥ 签名容差（24h），或把容差降到与缓存同量级 | 跨 6 分钟重放的同一签名必须被拒 |
| SEC-08 | 监控栈默认口令移除，改为启动必填 | `docker compose config` 在缺 env 时失败 |

> SEC-04 是**行为变更**（默认从"匿名"变"拒绝"），需在发布说明中标注；这也是"安全默认值"应有的方向。

> **落地状态（2026-10-22，本批次已实现，提交 `ca6980c51`）**。7 条落地的关键取舍：

| 条目 | 实际做法（与计划的差异） |
|---|---|
| SEC-02 | 计划"改环境变量 + `TEST_ONLY` 前缀"，实际改为「**缺失即失败**」（`${VAR:?msg}` 或显式 `exit 1`），不做静默降级——带默认值的"测试专用"口令一旦被复制到部署环境仍是弱口令。默认值直接删除，不做前缀标记。另外新增 `scripts/ci/check_no_default_admin_credential.sh` 并入 `ci.yml`，扫描面 = `git ls-files 'scripts/**' 'docker/**'` 排除 `*.md`（历史报告把凭据当证据引用，不是可执行代码）；脚本内模式写作 `Admin@12[3]` 以不自匹配 |
| SEC-03 | 计划只在 `validation.rs` 加校验，实际**还改了 `decode_secret`**：Base32 解码失败不再 `or_else` 回退原始字节（回退等于把非法密钥当合法用）；`.env.example` 的 `CHANGE_ME` 与测试 fixture `"mfa-secret"` 一并替换为 `JBSWY3DPEHPK3PXP` |
| SEC-04 | 计划"`PROMETHEUS_AUTH_DISABLED=true` 才匿名"，实际做成**三态枚举** `PrometheusAuth::{Token, Public, Disabled}`：有 token → 校验 Bearer；无 token 且 `prometheus.public=true` → 公开；否则 → 拒绝（fail-closed）。`public` 是配置文件字段（`#[serde(default)]`，默认 `false`）而非独立 env，避免"某个 env 忘了设"造成匿名。部署栈 `docker/config/homeserver.yaml` 显式 `public: true`——Prometheus 抓取不带认证头，且 9090 只绑 `127.0.0.1` |
| SEC-05 | 计划"按资源而非方法"，实际是 **写方法一律敏感 + 路径前缀 + 响应语义补充**。实测纠正原文一处错误：`GET .../appservices` 返回的 `AppServiceResponse` **不含** `as_token`，因此不纳入；真正返回明文凭据的是 `GET /_synapse/admin/v1/registration_tokens` |
| SEC-06 | 计划"仅可信代理采信 XFF"，实际把原规格**反转**：`peer_addr` 为 `None` 时直接返回 `None`（不再回退到转发头）。原用例 `no_peer_falls_back_to_header` 相应改名为 `no_peer_does_not_trust_forwarded_headers` 并断言 `None`——这是规格变更，不是为通过而改断言。另拆出 `deny_only_forwarded_ip`：只用于"拒绝本机以外"这类**加严**判定（`localhost_guard`、`admin/register`），无 peer 时仍需解析头，否则会把所有请求当成非本机 |
| SEC-07 | 计划"窗口对齐 24h"或"降容差"，实际让**窗口从容差推导**：`ReplayProtectionConfig::for_federation(enabled, signing_ts_tolerance_ms)`，`federation_window_secs()` = `max(容差, 300s)`，容量提到 `100_000`。单一真源，容差改了窗口自动跟随。跨实例层用 `CacheManager::try_acquire_lock("fed:replay:{sig_hash}")`（Redis `SET NX EX`）叠加在进程内缓存之上；Redis 故障时 **fail-open + `warn!`**（可用性优先，且不谎报"已防重放"） |
| SEC-08 | 计划"去掉默认值"，实际用 `:?` 必填展开：`${GRAFANA_ADMIN_PASSWORD:?GRAFANA_ADMIN_PASSWORD must be set (no default credentials)}`，`.env.monitoring.example` 置空并注释，4 处文档同步 |

**门禁（实测）**

| 门禁 | 结果 |
|---|---|
| `cargo test -p synapse-common --lib` | **992 passed / 0 failed**（含 SEC-03 4 条、SEC-07 2 条） |
| `cargo test -p synapse-web --lib -- utils::ip` | **26 passed / 0 failed**（含 SEC-06 规格反转后的用例与 `deny_only_forwarded_ip_works_without_a_peer`） |
| `cargo test -p synapse-web --lib -- admin_auth` | **22 passed / 0 failed / 1 ignored**（含 SEC-03 / SEC-05 共 5 条新用例） |
| `TEST_DB_ISOLATION=database cargo test -p synapse-rust --lib --features test-utils -- metrics_endpoint prometheus` | **7 passed / 0 failed**（含 SEC-04 5 条用例） |
| `check_no_default_admin_credential.sh` | OK。红态实测：向 `scripts/test/perf/run_tests.sh` 注入 `# probe: Admin@123` → exit 1；还原 → exit 0 |
| `docker compose config`（SEC-08） | 缺失/空 `GRAFANA_ADMIN_PASSWORD` → exit 1；有值 → exit 0 |

> 门禁归因：`check_file_size_ratchet.py` 仍红（1 个新超 600 行文件 + 24 条既有漂移）、`check_clippy_allow_ratchet.py` 仍红（1 条新增 + 3 条 stale），均为**并发会话改动 / 既有债务**，非本批次引入。W2+W5 只对触达的 4 个文件做了**手术式 baseline 上调**（`synapse-federation/src/signing.rs` 1202→1383、`synapse-web/src/middleware/federation_auth.rs` 1100→1128、`synapse-common/src/security.rs` 874→926、`synapse-storage/src/user_store_fake.rs` 706→739），未 `--update`，符合"暂不开棘轮清账任务"的决定。

---

### 10.8 W6：P3 冗余与文档（复用 §9.12 五批）

> **落地状态：批次 1（零风险）已完成（2026-10-23，提交 `e4ac05ce0`）**。范围 RD-02/03/04/05/06/23/24/25。实际落地与审计原文有 **3 处偏差**，以实际为准：
>
> ① **RD-03 的 `futures` 不能迁 `[dev-dependencies]`**：原文称 `synapse-web` 的 `futures` 仅测试使用；实测生产在 `synapse-web/src/middleware/write_forwarding/forward.rs:15,49`（`futures::Stream` / `futures::stream::…`）。本批次只迁 `serde_norway`、`deadpool-redis` 两条，**审计原文该结论作废**。
> ② **RD-04 的 `rsa` 必须同时保留 `[dev-dependencies]`**：`synapse-services/src/oidc_service.rs` 的测试模块直接 `use rsa::…`，只改成 `optional = true` 会让测试构建缺 crate。最终形态：`optional = true` + `builtin-oidc = […, "dep:rsa"]`（生产默认不编译）+ 同名 dev-dependency（测试可用）。
> ③ **RD-24 按原文建议不采纳**：`synapse-common/src/config/task_scheduler.rs` 的 `deserialize_max_concurrent_tasks` 现会拒绝 `0`/负数/浮点/bool/字符串；换成 `#[serde(default = …)]` 会静默接受 `max_concurrent_tasks: 0`，调度器并发度为 0 等同停摆，属**行为回归**。因此**不删自定义 deserializer**。
> （同日另做了一次**不改语义**的瘦身：去掉 `serde_json::Value` 中间层与多余的布尔分支、测试 7→5，`0` 拒绝保留 —— 见 §9.9 RD-24 的状态行。）

**批次 1 落地明细**

| 条目 | 实际改动 |
|---|---|
| RD-02 | `synapse-common`：删 `tower-http`、`http`、`validator`；`futures` 移入 `[dev-dependencies]` |
| RD-03 | `synapse-web`：`serde_norway`、`deadpool-redis` 移入 `[dev-dependencies]`；`futures` 保留（偏差 ①） |
| RD-04 | `synapse-services`：`rsa` 改 `optional = true` 挂到 `builtin-oidc` feature，并补同名 dev-dependency（偏差 ②） |
| RD-05 | 删除 `src/storage/mod.rs`（65 行纯 re-export 门面）；4 处消费方改直连 `synapse_storage`：`src/bin/synapse_worker.rs:15`、`src/bin/schema_health_check.rs:34`、`benches/performance_pagination_benchmarks.rs:18,129`、`benches/performance_sliding_sync_benchmarks.rs:47-50`；`src/lib.rs` 删 `pub mod storage`，`PresenceStorage` 重导出改向 `synapse_storage::presence::PresenceStorage` |
| RD-06 | `src/tasks/mod.rs` 删 8 个零调用 pub 访问器（`get_last_{health_status,performance_metrics,integrity_report,maintenance_report}`、`trigger_{health_check,performance_check,integrity_check,maintenance}`） |
| RD-23 | `synapse-common/src/config/push.rs` 由 6 行 re-export 壳改为承载 `PushConfig`/`ApnsConfig`/`FcmConfig`/`WebPushConfig` 的定义（原在 `voip.rs:121-186`）；`config/mod.rs` 重导出改向（`voip` 只留 `LivekitConfig`/`UrlPreviewConfig`/`VoipConfig`） |
| RD-25 | `synapse-web/src/routes/e2ee/backup.rs` 的文件级 `#![allow(clippy::unused_async)]` 收窄到 `verify_secure_backup_passphrase` 单函数，并补 `reason` |

**顺带修复（非本批次条目，但必须记录）**：`synapse-storage/src/push/mod.rs:1814` 的 `#[allow(clippy::panic)]` 缺 reason（并发会话引入，HEAD 上棘轮已红），补 `reason = "test-only: …"`；随后 `check_clippy_allow_ratchet.py --update` 把基线从 **163 条 / 98 文件收紧到 160 条 / 96 文件**（同时删除 3 条 stale 条目：`synapse-services/src/test_utils.rs`、`synapse-storage/src/test_utils.rs`、`synapse-test-utils/src/lib.rs`）。

**门禁（§9.12 批次 1 原文）**：`cargo check --workspace --all-targets --all-features` → `EXIT=0`（4m29s）；`cargo test --workspace --all-features --no-run` → `EXIT=0`；`check_clippy_allow_ratchet.py` → `OK: clippy allow suppression at baseline`。删掉 `src/storage/mod.rs` 后无未解析引用，即 §9.12 要求的"stage 人为删除后不得出现未解析引用"成立。

> **踩坑记录**：`--all-features` 不可省。`synapse-services/src/test_mocks.rs` 等模块被 `#[cfg(any(test, feature = "test-utils"))]` 门控，省略该 flag 时 default 构建会报 **331 条 `E0282`/`E0432`/`E0433` 假红**（本次实测复现），与本次改动无关。
> 另：`cmd | tail -n N` 的退出码是 `tail` 的，恒为 0；门禁一律用 `> /tmp/xx.log 2>&1; echo "EXIT=$?"` 取真实退出码。

> **落地状态：批次 2（需回归）已完成（2026-10-23，提交 `b51aafb00`）**。范围 RD-01/07/08/09/10/11/12/13。实际落地与审计原文有 **7 处偏差/补充**，以实际为准：
>
> ① **RD-01 口径修正为「1 删 + 14 迁 `[dev-dependencies]`」**：原文称 15 个依赖在 `src/**` 零引用因而可整体删除；逐条普查后发现其中 14 个仍被 `tests/**` 与 `src/**` 内的 `#[cfg(test)] mod tests` 直接 `use`（删掉会直接编译失败），真正的零引用只有外部 crate `config`（全仓 0 命中；`config::Config` 的命中全部是仓内 `synapse_common::config::Config`）。落地方案：删除 `config`；其余 14 个移入根 crate 的 `[dev-dependencies]`，同样实现"不进 `bin`/`lib` 生产构建图"的原始目的。
> ② **RD-07 无需改动**：`synapse-storage/src/lib.rs` 的 `user_store_fake` 模块门控与 re-export 门控已由 **W2 的 CODE-05** 落地（`synapse-storage/src/lib.rs:163-164`、`:233-234`），本批次未重复修改。
> ③ **RD-08 的 re-export 门控原本就在位**：`synapse-common/src/lib.rs:196` 已是 `#[cfg(any(test, feature = "test-utils"))]`；本批次实际只补**模块声明**门控（`synapse-common/src/lib.rs:46-50`）与根 crate `src/common/mod.rs:27-28` 的转发门控，并给根 crate 的 `test-utils` feature 显式追加 `"synapse-common/test-utils"`。原文建议的"`generate_test_keypair` 改走 `synapse-test-utils`"**未采纳**：该二进制早已有 `#[cfg(feature = "test-utils")]` 的双 `main` 形态（有门禁时生成密钥，无门禁时打印用法并 `exit(1)`），改走 `synapse-test-utils` 反而要给库 crate 增加反向依赖。
> ④ **RD-10 的实现细节与原文不同**：`privacy_storage` 从 `ExtensionServices` 字段改为 `container.rs` 的构造期局部变量；但 `PrivacyStorage::new` 的签名是 `pub fn new(pool: Arc<Pool<Postgres>>) -> Self`，必须传 `infra.infra.pool.clone()` 而非 `&infra.infra.pool`（后者会 `E0308`）。
> ⑤ **RD-11 推翻了 2026-10-09 的 by-design 裁定**：`docs/前缀命名空间治理方案-2026-10-08.md:3008-3015` 曾以"顶层 `deny_unknown_fields` ⇒ 删除 `[livekit]` 会让既有配置启动失败"为由裁定保留 `Config.livekit`；该文档 `:3281` 同时允许"同步删键 + 迁移说明"这条替代路径。本轮核实全仓 YAML/样例配置**无** `livekit:` 键，按 2026-10-22 审计 RD-11 执行**整体删除**（`LivekitConfig` 结构体 + `Config.livekit` 字段 + 全部 re-export）。**迁移说明：外部部署若在自己的 `homeserver.yaml` 里写了 `[livekit]` 段，升级后会在启动期报 `unknown field`，必须一并删除该段。**
> ⑥ **RD-12/RD-13 的迁移语义必须写进文档，否则运维会以为"配了就生效"**：`Config` 顶层有 `deny_unknown_fields`，但 `ServerConfig`/`LoggingConfig` 等**嵌套结构体没有**——因此删除嵌套字段后，既有配置里的 `form_secret` / `soft_file_limit` / `user_agent_suffix` / `max_image_resolution` / `warmup_pool` / `log_dir` 会被 serde **静默忽略**（不会启动失败），删除顶层 section（如 `livekit`）则会**报错**。两种行为都已由新测试 `test_removed_config_keys_are_rejected_or_ignored_as_documented` 锁定。
> ⑦ **`FORM_SECRET` 的部署面遗留（无害，但记录在案）**：配置已不再读取该键，但 `docker/deploy/docker-compose.yml:265`、`docker/deploy/.env.example:36`、`docker/deploy/deploy.sh:1104`、`docker/deploy/scripts/generate-secrets.sh:91,365`、`scripts/validate_config.sh:14`、`.env.example:59`、`README.md:53` 等仍在生成/校验/透传该 env。多余 env 不被读取即无影响，故本批次不动部署面（`docker/federation-test/synapse/entrypoint.sh:116` 的 `form_secret:` 属 Python Synapse 对照容器，**禁止**删除）。

**批次 2 落地明细**

| 条目 | 实际改动 |
|---|---|
| RD-01 | 根 `Cargo.toml`：`[dependencies]` 删 15 个（`async-trait`/`tower`/`hyper`/`serde_norway`/`rand`/`sha2`/`hmac`/`config`/`futures`/`regex`/`url`/`urlencoding`/`validator`/`image`/`hex`），其中 14 个同名移入 `[dev-dependencies]`（`sha2` 保留 `features = ["oid"]`），仅 `config` 真删；`test-utils` feature 追加 `"synapse-common/test-utils"` |
| RD-07 | 无需改动（W2 CODE-05 已完成，偏差 ②） |
| RD-08 | `synapse-common/src/lib.rs`：`federation_test_keys` 模块加 `#[cfg(any(test, feature = "test-utils"))]`；`src/common/mod.rs` 转发同步加门控（偏差 ③） |
| RD-09 | `synapse-services/src/wiring/admin/models.rs` + `admin.rs`：删 14 个 storage 字段及对应 struct literal（`refresh_token_storage`、`registration_token_storage`、`federation_blacklist_storage`、`media_quota_storage`、`captcha_storage`、`audit_storage`、`feature_flag_storage`、`event_report_storage`、`background_update_storage`、`retention_storage`、`push_notification_storage`、`app_service_storage`、`app_service_event_reader`、`worker_storage`）；构造期局部变量保留（仍被对应 service 消费），`AdminServices` 上方"留给未来路由"注释改为 RD-09 说明块 |
| RD-10 | `synapse-services/src/wiring/extensions.rs`：删 `privacy_storage` 字段/局部变量/literal 项；`synapse-services/src/container.rs` 以 `PrivacyStorage::new(infra.infra.pool.clone())` 构造（偏差 ④） |
| RD-11 | 删 `synapse-common/src/config/voip.rs` 的 `LivekitConfig`、删 `Config.livekit` 字段与 3 处 `LivekitConfig::default()`、删 `synapse-services/src/lib.rs:240` 与 `rtc/mod.rs` 的 re-export；`synapse-services/src/test_config.rs`、`tests/integration/transaction_tests.rs`、`src/common/config/tests.rs` 同步删 literal（偏差 ⑤） |
| RD-12 | 删 `ServerConfig` 的 `form_secret`（含 `#[educe(Debug(ignore))]` + 6 行 doc）、`soft_file_limit`、`user_agent_suffix`、`max_image_resolution`、`warmup_pool`（含 `default_warmup_pool()`）；删 `loader.rs` 的 `form_secret`/`user_agent_suffix` env 解析；删 `docker/config/homeserver.yaml`、`scripts/run_bench_server.sh`、`synapse-common/src/config/server.rs` 两处测试 YAML 样例中的对应键 |
| RD-13 | `synapse-common/src/config/logging.rs` 的 `LoggingConfig` 收窄为 `level`/`format`/`log_file` 三项；删 `loader.rs:91` 的 `log_dir` 解析；`test_logging_config_with_file` 改为只断言 `log_file`（原 `log_dir` 断言一并删除，因字段已不存在）；`src/common/config/tests.rs`、`synapse-services/src/test_config.rs`、`tests/integration/transaction_tests.rs` 删 `log_dir: None` |
| 新增反向测试 | `synapse-common/src/config/mod.rs`：`test_removed_config_keys_are_rejected_or_ignored_as_documented` —— 断言顶层 `livekit` 被 `deny_unknown_fields` 拒绝（错误信息含 `unknown field`）、嵌套残留键被静默忽略（`ServerConfig`/`LoggingConfig` 反序列化均 `Ok`），即 §9.12 要求的"配置字段存在性"反向门禁 |
| RD-26（顺带，原排后续批次） | `LivekitConfig` 整体删除后，三处"死配置证据"的表述已失效，就地修正为"已删除"：`docs/synapse-rust/API_COVERAGE_REPORT.md:416`、`docs/synapse-rust-vs-synapse-comparison.md:654,683`（不再引用 `config/voip.rs:92` 的 `ws_url` 字段） |

**门禁（§9.12 批次 2 原文）**：`cargo check --workspace --all-targets --all-features` → `EXIT=0`；`cargo nextest run --workspace` 全量 → 见下；"配置字段存在性"反向测试已落地为 `test_removed_config_keys_are_rejected_or_ignored_as_documented`（PASS）。

- **默认 feature 的生产构建图**（RD-01/RD-08 的真正盲区：`--all-features` 会顺带打开 `test-utils`，看不出依赖是否泄漏进 `bin`/`lib`）：`cargo check --workspace --lib --bins --examples` → `EXIT=0`、`warning` 计数 0（3m26s）。
- **全量回归**（`E2EE_INTEROP=1 cargo nextest run --workspace --all-features --no-fail-fast`，5,501s）：`10735 tests run: 10718 passed, 17 failed, 16 skipped`。17 条逐条甄别：
  - **16 条与 §10.6（W4 第三层）登记的失败集完全相同**（`api_key_backup_route_table_tests`、`api_placeholder_contract_p1p2_tests`、`api_relations_authorization_tests`、`api_route_ledger_tests`×2、`sqlx_cache_tooling_guard_tests`×2、`sqlx_ratio_gate_tests`×4、`sqlx_dynamic_literal_guard_tests`、`test_ddl_guard_tests`、`monitoring::db_tests`、`worker::tests`、`api_beacon_location_tests`(抖动)），均 HEAD 既有或环境侧，均可在纯 HEAD 工作树复现。
  - **1 条是批次 1 的连带回归，已在本批次内修掉**：`placeholder_scan_tests::test_empty_json_successes_are_allowlisted` 报 `e2ee/backup.rs:177` 未登记。根因是批次 1 的 RD-25 把 `synapse-web/src/routes/e2ee/backup.rs` 的文件级 `#![allow(clippy::unused_async)]` 收窄到单函数，文件净减 2 行，`Ok(empty_json())` 从 `:179` 移到 `:177`，而 `scripts/shell_routes_allowlist.txt` 是**行号键**（其自身文档就写明"fix the line number if the site moved"）。按该口径把条目改成 `:177` 后 `test(placeholder_scan_tests)` 4/4 绿。**教训**：批次 1 的门禁只跑了 `cargo check` + `test --no-run`，没跑全量 nextest，行号型守卫因此漏网——凡改动 `synapse-web/src/routes/**` 的行数，都必须跑 `placeholder_scan_tests`。
- **sqlx 动态 SQL 棘轮的 3 项违规归因（非本批次引入，不修）**：`check_sqlx_dynamic_ratio` 报生产动态 `133 → 136`、测试基础设施 `767 → 779`、静态 `1367 → 1366`。本批次**零 SQL 改动**（`container.rs` 唯一的 `sqlx::query` 命中是注释）。用 `git archive` 分别导出 HEAD 与基线提交 `1ced60cd5` 跑同一个 census，得到 HEAD `136/779/1366`、`1ced60cd5` `133/767/1367`，逐点差集落在 `synapse-storage/src/invite_blocklist.rs`（6 测试 + 2 生产）、`synapse-storage/src/media/chunked_upload.rs`、`synapse-services/src/media/tests.rs`、`synapse-storage/src/event/db_tests.rs` —— 即 §10.6 已登记的"HEAD 既有计数漂移（纯 HEAD 工作树同样红）"，与 W6 无关。棘轮基线**不下调**（只能因修复而收紧），此项作为独立"棘轮清账"任务挂账。

> **落地状态：批次 3（契约变更）已完成（2026-10-23，提交 `f7bf4eec0`）**。范围 RD-15…RD-19。实际落地与审计原文有 **5 处偏差/补充**，以实际为准：
>
> ① **审计漏了 2 条恒定失败端点，静态扫描补出**：`GET /_matrix/client/v3/rooms/{room_id}/keys/distribution`（`e2ee/devices.rs:102` 恒 403，自述"server-internal endpoint"）与 `POST /_matrix/client/v3/keys/backup/secure/{backup_id}/verify`（`e2ee/backup.rs:153` 恒 410 `gone`，文件内已有 `// by-design: stub` 注释）。二者与 RD-15…RD-19 同性质，一并登记。**因此本批实际登记 16 条，而非审计列的 14 条**（RD-15:4 + RD-16:5 + RD-17:1 + RD-18:1 + RD-19:3 + 补充 2）。
> ② **RD-15/16/17 一律选「登记」，不选「下架」**：下架会把"契约合法但不支持"（404 `M_UNRECOGNIZED` / 501 `M_UNKNOWN`）退化成"路由不存在"（405 + `Allow` 头），反而让 bridge 客户端失去可区分的错误语义，也让 §9.12 的 ledger 探针失去观测面。RD-15 原文建议的"保留并登记"即此路径；RD-16 原文的"若确认无对端依赖则移除"**未采纳**——本批次无法证明"无对端依赖"，而登记 + 双向守卫已消除"悄悄复活/悄悄腐烂"的风险。
> ③ **RD-18 选「score 用于日志」而非「改用 `Value`」**：删掉 `UpdateReportScoreBody` 会让 `MatrixJson` 退化为不校验的 `Value`，请求体形状不再受 serde 约束；改为在拒绝前把 `event_id` / `requester_user_id` / `requested_score` 记入 `tracing`，既消掉 `#[allow(dead_code)]`，又留下"谁在调这个不可用接口"的运维证据。
> ④ **「有意不实现」清单是生成物，不是手写段落**：`scripts/contract/gen_contract_doc.py` 直接读 `ledger_annotations.txt` 的 `unsupported=true` 派生表格并写入 `ROUTE_CONTRACT.md`，因此清单与注解表**不可能漂移**。生成段落刻意避开「注册路由条目」字样——`check_route_surface_reconciliation.py` 会把该短语后的数字当总条数校验，误用即红。
> ⑤ **`API_COVERAGE_REPORT.md` 只加「指针 + 门禁说明」（§6.4），不抄 16 条明细**：抄写必然漂移；条目数的唯一来源是 `ROUTE_CONTRACT.md` 的生成段与 `ROUTE_HANDLERS.json` 的 `counts.unsupported`（本批 4 → **16**）。

**批次 3 落地明细**

| 条目 | 实际改动 |
|---|---|
| RD-15 | `scripts/contract/ledger_annotations.txt` 的 4 条 thirdparty（`/thirdparty/{location,user}[/{protocol}]`）在既有 `query_params=` 行上追加 `unsupported=true`（同 `(method,path)` 多行的键必须不相交，故直接续写而非新起一行） |
| RD-16 | 联邦 5 条登记：`POST /_matrix/federation/v1/user/keys/upload`、`POST /_synapse/federation/v1/keys/{upload,claim,query}`、`GET /_synapse/federation/v1/query/auth` |
| RD-17 | 无需改代码；`GET /_matrix/client/v3/rooms/{room_id}/event/{event_id}/keys` 早已在册（`ApiError::not_supported` → 501 `M_UNKNOWN`），本批仅把它纳入守卫覆盖 |
| RD-18 | `synapse-web/src/routes/directory_reporting.rs`：删 `UpdateReportScoreBody.score` 的 `#[allow(dead_code)]`；`_auth_user`/`_body` 改为实参并在 `forbidden` 前 `tracing::info!(event = "report_score_update_rejected", …)` |
| RD-19 | `synapse-web/src/routes/voice.rs`：`"max_duration_ms": 60_0000` → `600_000`（同值，仅消歧义）；三端点登记 `unsupported=true` |
| 新增守卫 | `scripts/contract/check_unsupported_endpoints.py`：静态剥注释/字符串后取函数体最后一条顶层语句，判定是否恒为 `Err(...)`（证据 = 构造函数名），与 `unsupported=true` **双向**比对；带 `--self-test`（11 例）。接入 `scripts/contract/check_route_contract.sh` |
| 契约生成 | `scripts/contract/gen_contract_doc.py` 新增「有意不实现的端点（by-design unsupported）」段（从注解表派生 + 条目数 + 表格）；`docs/synapse-rust/ROUTE_CONTRACT.md` 重新生成 |
| 镜像同步 | `scripts/contract/extract_registered.py` 的 `EXPECTED_ANNOTATIONS` 逐条镜像新增注解（`check_annotation_fidelity` 第 1 层要求表 == 镜像，逐键逐值），否则 `EXTRACT_STRICT=1` 直接 `SystemExit` |
| 文档 | `docs/synapse-rust/API_COVERAGE_REPORT.md` 新增 §6.4「有意不实现」（指针 + 双向门禁 + 错误码语义，不抄明细） |

**门禁（§9.12 批次 3 原文：「`api_route_ledger_tests` + 契约文档与代码的路由集合比对脚本」）**

| 门禁 | 结果 |
|---|---|
| **红证**（先证守卫能失败） | 补注解前运行新守卫 → `EXIT=1`，逐条报出 **12** 条「handler 恒失败但未登记」（RD-15 4 + RD-16 5 + RD-18 1 + 补充 2，RD-17/RD-19 已在册故不报） |
| `check_unsupported_endpoints.py --self-test` | `EXIT=0`：11 个分类器/守卫用例全绿 |
| `check_unsupported_endpoints.py` | `EXIT=0`：`✅ 16 registered route(s) declared by-design unsupported; 16 constant-failure handler(s) match the declaration` |
| `EXTRACT_STRICT=1 extract_registered.py` | `EXIT=0`：`check_annotation_fidelity` 七层校验通过；`ledger_export` **913** / `ledger_export_sdk` **975** 与 fixture 一致 |
| `gen_derived_routes.py --check` / `gen_route_handlers.py --check` | `OK`；sidecar `counts.unsupported` = **16** |
| `bash scripts/contract/check_route_contract.sh` | 除最后一步「`ROUTE_CONTRACT.md` 相对 HEAD 有 drift」（本批新生成段落尚未提交，属预期）外全绿；提交后复跑应为 `EXIT=0` |
| `cargo check --workspace --all-features --lib --bins` | `EXIT=0`，0 error / 0 warning |
| `cargo nextest -E 'test(/route_ledger\|api_route_ledger/)'` | 22 tests：20 passed / 2 failed。2 条均为 `declared_route_manifest_full_snapshot_matches_{default,worker_enabled}_state`，**非本批引入**（详见下方归因） |
| 全量回归（`E2EE_INTEROP=1 cargo nextest run --workspace --all-features --no-fail-fast`） | `EXIT=100`（3,606s）：`10735 tests run: 10720 passed (10 slow), 15 failed, 16 skipped`。**15 条全部落在 §10.6 已登记的 W4 既有失败集内，零新增**（详见下方「全量回归」段） |

- **2 条快照失败的归因（本批不修、不吞）**：用 `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` 生成后 `git diff` 显示差异**只有 SAML/CAS 的 28 条路由**（`count: 928 → 956`，`[_synapse/admin/v1/cas/*]`、`[_synapse/admin/v1/saml/*]`、`[_matrix/client/v3/login/sso/redirect/*]` 等），与 RD-15…RD-19 **零交集**；本批未改任何路由注册。即 HEAD 上这两份快照早已落后于代码（属 §10.6 登记的 W4 既有失败集 `api_route_ledger_tests`×2）。**快照已还原未提交**——把它一并"修绿"会把其他会话的路由变更混进本批，且掩盖真实的 drift 信号。
- **踩坑**：`ledger_annotations.txt` 要求「**恰好一个 tab**」（多一个/少一个即 `SystemExit`），手工扩列对齐极易写成空格；同一 `(method, path)` 可多行但**键必须不相交**（重叠即硬失败），因此追加 `unsupported=true` 要续在既有行上。
- **全量回归**（`E2EE_INTEROP=1 cargo nextest run --workspace --all-features --no-fail-fast`，3,606s）：`10735 tests run: 10720 passed (10 slow), 15 failed, 16 skipped`，`EXIT=100`。15 条逐条甄别，**全部命中 §10.6（W4 第三层）已登记的失败集，零新增**：
  - 契约/路由面 4 条：`api_route_ledger_tests::declared_route_manifest_full_snapshot_matches_{default,worker_enabled}_state`（即上文 SAML/CAS 快照 drift）、`api_key_backup_route_table_tests::key_backup_routes_are_wired_under_v1_and_r0`、`api_relations_authorization_tests::relations_event_type_route_filters_over_http`、`api_placeholder_contract_p1p2_tests::test_power_levels_contract_rejects_non_members`（后 3 条为 HEAD 既有的占位/契约类红，本批未触及对应 handler）。
  - sqlx 棘轮与 DDL 守卫 8 条：`sqlx_ratio_gate_tests`×4、`sqlx_cache_tooling_guard_tests`×2、`sqlx_dynamic_literal_guard_tests`、`test_ddl_guard_tests`（§10.6 已归因为 HEAD 既有计数漂移，基线不下调）。
  - 环境/工具链 2 条：`synapse-storage monitoring::db_tests::database_monitor_round_trip_on_the_migration_template`、`synapse-storage worker::tests::test_worker_type_owned_route_prefixes`。
  - 相比批次 2 的 17 failed，本次 **−2**：`placeholder_scan_tests::test_empty_json_successes_are_allowlisted`（批次 2 内已修）、`api_beacon_location_tests`（抖动，本轮通过）；**无任何条目因本批从绿转红**。

直接执行 [§9.12 分阶段清理建议](#912-统计与分阶段清理建议)，不重复。补两条本轮的：

| 条目 | 改动 | 门禁 |
|---|---|---|
| CODE-12 | 删除 `preview.rs:71` 死表达式 | `cargo clippy -p synapse-web -- -D warnings` |
| CODE-10 / CODE-11 | 只加"危险性扫描"（缓存/存储删除禁止 `let _`）+ `// reason:` 强制；**不设总数下降配额** | `check_clippy_allow_ratchet.py` 扩展后故意新增一条无 reason 的 allow 必须红 |
| TEST-07 / DOC-02 / DOC-09 | 统一为 283（128/147/3/5）；数字由脚本输出，文档只引用 | 文档数字与脚本输出不一致时 CI 红（需新增 `check_doc_metrics.py`） |
| PERF-12 / DOC-07 | `INDEXES.md` 由 SQL 解析生成 | 生成结果与文件不一致时 CI 红 |

---

### 10.9 门禁总表（落地时按此勾选）

| # | 门禁 | 现状 | 由哪一批引入 |
|---|---|---|---|
| 1 | `cargo build --workspace --all-targets` | 已有 | 全部 |
| 2 | `cargo nextest run --workspace` | 已有 | W0/W1/W2 |
| 3 | `scripts/ci/check_clippy_allow_ratchet.py` | 已有 | W6 扩展 |
| 4 | `check_missing_docs_ratchet.sh` | 已有 | W6 |
| 5 | `api_route_ledger_tests` | 已有 | W1/W4（契约变更） |
| 6 | `no_silent_cache_delete` 扫描 | **W0 已落地**：`scripts/ci/check_no_silent_cache_delete.sh`，已接入 `ci.yml` | W0 |
| 7 | `EXPLAIN` 计划快照 | **新增** | W4 |
| 8 | 文档数字一致性（测试数/索引数） | **新增** | W6 |
| 9 | `check_feature_matrix.py` 接入 CI | 脚本存在但**未接入** | W6（RD-14） |
| 10 | `no_default_admin_credential` 扫描 | **W5 已落地**：`scripts/ci/check_no_default_admin_credential.sh`，已接入 `ci.yml` | W5（SEC-02） |

> 第 6/7/8 条是本次方案的**关键增量**：前三轮审查暴露的根因不是"没发现问题"，而是"修完无法证明修好了"。这三条把"一次性的审查结论"变成"持续可失败的门禁"，否则下一轮还会重新审出同样的条目。

---

### 10.10 明确不做（防止过度开发）

| 不做 | 理由 |
|---|---|
| 为 CODE-10 的总数设下降配额 | 大部分是 `unwrap_or_default()` 的合法用法，配额会诱导写 `unwrap()` 来"减少计数" |
| 重写 clippy allow 棘轮 | 它已经在工作（CODE-11 第二轮已更正） |
| 拆分测试文件（`db_tests.rs` 等 3 个 >2000 行） | 测试文件不承担生产职责，拆分的收益（并行编译）远低于冲突成本 |
| 删除 voice 三端点 / `/_synapse/admin/v1/backups` | MSC3245 与"有意不实现"，只登记不下架（FUNC-02、FUNC-05） |
| 把 `sync` 与 `sliding_sync` 强行合并 | ARCH-02 已改述为"共享语义基座缺失"，合并是重写而非重构 |
| 在 W3 决策前动 Thread 代码 | 三个方案代价相差一个数量级，先采集用量再决策 |

---

### 10.11 验收口径（怎么算"这一轮收口了"）

一轮审查的收口标准不是"条目清零"，而是**这四条同时成立**：

1. **P1 = 0 且新增 P1 有门禁拦截**：W0–W3 完成后，重新跑本文件的 P1 清单，必须 0 命中；且门禁表第 6 条能拦住同类回归。
2. **每个已修条目都有"先红后绿"的记录**：提交历史里能看到测试先失败的 commit（或 PR 描述中贴红/绿两次输出）。
3. **度量可复现**：本文件所有数字（268/117、905/244、1593/337、283、347）都能用文中给出的命令一次跑出来；跑不出来就说明口径又漂了。
4. **未覆盖项被显式承认**：附录 B 的口径保持——没跑的 benchmark、没跑的全量集成套件，继续写"未覆盖"，不拿静态推断冒充实测。

> 建议把本节作为下一轮审查的**输入模板**：下一轮只需在 §0.2 追加一节同样格式的表格，并复核 §10 各批次的完成情况。
