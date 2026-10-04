# synapse-rust 遗留问题排查报告

> 基线版本：v6.2.0
> 分支：`feature/2026-10-01-metrics-docs-updates`（`2609889af`，工作区干净）
> 排查日期：2026-10-04
> 结论：**无 P0 阻断项，31 项遗留问题（P1×5 / P2×14 / P3×12）**

---

## 一、排查方法与范围

| 手段 | 执行情况 | 结果 |
|------|----------|------|
| 代码审查 | 9 包 workspace 全量 + 6 领域并行子代理 | 完成，文件:行号级证据 |
| 静态门禁 `cargo clippy --workspace --all-targets --all-features` | 已运行 | **0 警告，exit 0**（12m17s） |
| 依赖安全 `cargo audit` | 已运行 | **干净**，624 依赖无已知漏洞 |
| `unsafe` 棘轮（geiger） | 读取基线 | 生产 **0**，测试 9 |
| 债务标记扫描 | TODO/FIXME 11、`unimplemented!/todo!` 13、`unwrap/expect` 887 | 生产实质缺口仅 1 处 |
| 配置/部署审查 | homeserver.yaml、rate_limit.yaml、deploy.sh | 完成 |
| 文档死链/口径审查 | docs/（>200 份 Markdown） | 完成 |
| 运行时验证 | 上一轮 `deploy.sh --all` 部署 | 23 步成功、容器 healthy、健康检查全 200、DB 一致性通过 |

**方法学边界**：本轮为「代码审查 + 静态工具 + 配置审查 + 既有部署健康验证」，**未**重跑完整功能/集成/性能基准测试；标注为「潜在」的项为静态推断，需运行期确认。

---

## 二、总体结论（正面基线）

- **编译/静态质量已达发布级**：clippy 全绿（含 `unwrap_used/expect_used = deny` 门禁）、audit 无漏洞、生产 `unsafe = 0`。
- **核心安全链路扎实**：JWT（HS256+exp/iat/sub+iss/aud）、refresh token CAS 轮换 + 重用全撤销、SQL 全参数绑定、SSRF 防护、安全响应头齐全、Server ACL fail-closed。
- **无 P0**：不存在阻断发布的缺陷；问题集中在**运行时内存放大、文档可信度、部分功能未接线**三类。

---

## 三、优先级总览

| 级别 | 数量 | 主题 |
|------|------|------|
| **P1 高** | 5 | 指标内存无界增长（B1）、内容扫描路径硬编码（A2）、文档路径漂移/死链/监控文档互斥/基线口径冲突（F1/F2/F3/F5） |
| **P2 中** | 14 | 功能未接线（A1/A3/A4）、缓存 O(n)/无 LIMIT 查询（B2/B3）、安全 fail-open 与授权降级（C1–C4）、SDK 文档不符（D1–D3）、`#[allow]` 债务（E1） |
| **P3 低** | 12 | 次要性能/配置/注释/文档噪音（B4/B5/B7、C5/C6、D4、E2–E6、F4/F6/F7/F8） |

---

## 四、分领域详细清单

### A. 功能完整性与正确性（代码缺陷）

**A1｜`m.delayed_event` EDU 只校验不持久化** — 严重度 中｜P2
- 证据：`synapse-web/src/federation/edu.rs:761-822`（`handle_delayed_event_edu`，L806 `// TODO: P1-1`、L818 日志明写 `NOT persisted yet`）
- 影响：联邦入站的 MSC4140 延迟事件同步不完整；指标 `..._processed_total` 口径为"已接收"可能被误读为已落库
- 建议：接入 `delayed_event_service`（已有模板 `docs/templates/federation-edu-persist-template.md`）

**A2｜ClamAV socket 路径硬编码** — 严重度 中｜P1
- 证据：`synapse-services/src/content_scanner/service.rs:55`（`"/var/run/clamav/clamd.sock"`），无视 `synapse-common/src/content_scanner/mod.rs:97` 的 `clamav_socket_path`
- 影响：非默认部署路径下扫描 fail-closed → **媒体上传被拒**
- 建议：从 `ContentScannerConfig` 透传 socket 路径

**A3｜voice 三端点恒返回 501** — 严重度 中｜P2
- 证据：`synapse-web/src/routes/voice.rs:323,334,345`（convert/optimize/transcribe）
- 建议：实现或从路由移除并文档标注

**A4｜事务幂等非原子** — 严重度 中｜P2
- 证据：`synapse-services/src/room/messaging/messages.rs:328-348`（`begin_txn` L336 → `send_message` 内部 `tx.commit()` L271 → `finish_txn` L346）
- 影响：两者不在同一事务，崩溃窗口可致**重复事件**
- 建议：合并为单一事务

**A5｜admin 媒体端点路径大小写不一致** — 严重度 低-中｜P3
- 证据：`docs/synapse-rust/ROUTE_CONTRACT.md:479-497`（`:497 user` vs `:489 users`）

### B. 性能与稳定性

**B1｜指标直方图内存无界增长** — 严重度 中-高｜**P1（本报告最高优先代码项）**
- 证据：`synapse-common/src/metrics.rs:157-190`（`Histogram.values: Arc<Mutex<Vec<f64>>>`，`observe` L187-190 每次 `push` 无上限/TTL；生产无周期 reset）
- 影响：**唯一随运行时长单调增长的内存放大点**，长跑实例 OOM 风险
- 建议：改为租约式分桶计数，或加容量上限 + 周期快照后清空

**B2｜缓存前缀操作 O(n) 全量扫描** — 中｜P2
- 证据：`synapse-cache/src/manager.rs:216,237`（`get_keys_with_prefix` / `invalidate_local_pattern`）

**B3｜大表查询无 LIMIT** — 中｜P2
- 证据：`synapse-storage/src/event/state.rs:296,340`（`get_state_events_batch` / `get_membership_state_keys_since_batch`）

**B4｜全表 COUNT(\*)** — 低-中｜P3
- 证据：`synapse-storage/src/event/basic.rs:165,176`（`get_total_message_count`）

**B5｜按 origin 的 Semaphore HashMap 无驱逐** — 低-中｜P3
- 证据：`synapse-web/src/routes/federation/transaction.rs:963-974`

**B6｜构建慢（工程效率）** — 中｜——
- 说明：release profile `lto=true/codegen-units=1` 致镜像构建 ≈28 分钟（观测值）

**B7｜media get_stats 在 async 内同步递归 IO** — 低（当前仅测试可达）｜P3
- 证据：`synapse-storage/src/media/filesystem.rs:211,225,234`

> 正面：规则 §4.3 必需索引全部存在；连接池自洽（max_size=50）；无 `SELECT *`/N+1/连接泄漏。

### C. 安全性

**C1｜key_rotation 绕过 RBAC/MFA** — 中｜P2
- 证据：`synapse-web/src/routes/key_rotation.rs:273-294` 仅用 `ensure_server_admin`（`synapse-web/src/utils/admin_auth.rs:36-42`，只查 `is_admin`）。注释标明为**有意设计**，但密钥轮换属高权限操作，建议改走完整授权路径

**C2｜登录锁定 Redis 故障时 fail-open** — 中｜P2
- 证据：`synapse-common/src/config/security.rs:123-126` 默认 `true`

**C3｜限流 fail-open** — 中｜P2
- 证据：`docker/config/rate_limit.yaml:75` `fail_open_on_error: true`（覆盖代码默认 false）→ Redis 故障时全局限流失效

**C4｜TURN 弱默认密钥** — 中｜P2
- 证据：`docker/config/homeserver.yaml:229` `dev-turn-secret`

**C5｜联邦签名允许明文 & master key 可空** — 低-中｜P3
- 证据：`docker/config/homeserver.yaml:143-144`

**C6｜密码重置邮箱端点缺专门限流** — 低｜P3
- 证据：`docker/config/rate_limit.yaml`（`:22-56` 敏感端点规则中无重置邮箱端点）

> 正面：JWT 校验完整、refresh 重用检测、SSRF 全链路、AEAD 密钥加密、生产 unsafe=0。

### D. 兼容性与可访问性

**D1｜SDK 错误文档格式不符** — 中｜P2
- 证据：文档称 `{status, code, message}`（`docs/sdk/errors.md:26-45`），实现统一 `{errcode, error}`（`synapse-common/src/error.rs:930-933`）

**D2｜SDK README 版本/前缀陈旧** — 中｜P2
- 证据：版本 `0.1.0` vs Cargo `6.2.0`；使用 `r0/` 旧前缀（`docs/sdk/README.md:12,41-52`）

**D3｜`event_id.rs` 头注释与实现矛盾** — 中｜P2
- 证据：称"deliberately not wired"（`synapse-common/src/event_id.rs:1-10`），实际已由 `synapse-federation/src/event_finalize.rs:67-69` 接线

**D4｜撤销级联口径冲突** — 低｜P3
- 证据：实现（`synapse-web/src/routes/handlers/room/events.rs:1134-1169`）与 `docs/synapse-rust/API_COVERAGE_REPORT.md:491` 不一致

> 正面：错误响应符合 Matrix 规范、安全头齐全、ACL fail-closed、v1/v2 与 v3+ event_id 处理正确。

### E. 代码质量与技术债务

| 编号 | 问题 | 严重度 | 证据 |
|------|------|--------|------|
| E1 | `#[allow(clippy::)]` ≈160 处（含 61 处 `too_many_arguments`） | 中｜P2 | 全仓 |
| E2 | `create_event` ≈194 行超长函数 | 低-中｜P3 | `synapse-services/src/room/messaging/events.rs:198-392` |
| E3 | `src/e2ee/`+`src/cache/` 多层 facade 冗余 | 低-中｜P3 | — |
| E4 | `vendor/pastey` fork 不受门禁 | 低｜P3 | `vendor/` |
| E5 | 债务标记 11 + 未实现 13 + unwrap/expect 887（多为测试） | 低｜P3 | — |
| E6 | `.clippy.toml` 阈值放宽（cognitive-complexity=25 等） | 低｜P3 | `.clippy.toml` |

### F. 文档与知识库

| 编号 | 问题 | 严重度 | 证据 |
|------|------|--------|------|
| **F1** | 路径漂移：`src/web`/`src/services` → 实为 `synapse-web/src`/`synapse-services/src` | **高｜P1** | `CONTRIBUTING.md:70-72`、`AGENTS.md:400`、`docs/INDEX.md:5,26,40`、`docs/audit/sdk-encapsulation-audit.md:472,488,542` |
| **F2** | `docs/INDEX.md:78-82` 引用 5 个不存在的 `NN_*.md` 审计文件 | **高｜P1** | `docs/INDEX.md:78-82` |
| **F3** | 两套监控栈文档互斥指令 | **高｜P1** | `README_MONITORING.md:6` vs `MONITORING_GUIDE.md` / `QUICKSTART.md:79` |
| **F5** | 迁移基线口径冲突：CHANGELOG 称 v10，实际 v12 | **高｜P1** | `CHANGELOG.md:8` vs `migrations/README.md:54` |
| F4 | 死链：README.md:170/263/266、TESTING.md:701-704、CONTRIBUTING.md:130 | 中｜P2 | — |
| F6 | 文件名与内容不符（`overview.md`、`QUICKSTART.md`） | 中｜P2 | — |
| F7 | `docker/deploy/README.md` 漏 6 个选项、流程与脚本不符 | 中｜P2 | `docker/deploy/README.md:72-91` |
| F8 | 文档噪音：audit 62 / archive 41 / plans 36 份 | 低｜P3 | `docs/` |

---

## 五、重要订正：既有审计文档的失效结论

本次复核**证伪**了仓库内多份既有文档的结论，使用它们时须警惕：

| 既有结论 | 实际（本次证据） |
|----------|------------------|
| Content Scanner"空转" | 已接线：`synapse-web/src/routes/media/upload.rs:89,137`、`synapse-web/src/routes/handlers/room/events.rs:309-317` |
| 缩略图 `animated` "MISSING" | 已全链路实现：`synapse-services/src/media_service.rs:492-499,549-574,630-717` |
| MSC3912 客户端撤回"不级联" | `with_rel_types` 非空时**会**级联（`synapse-web/src/routes/handlers/room/events.rs:1134-1169` + `synapse-services/src/event_redaction_service.rs:120-192`） |
| P0-2 event_id reference hash"待解决" | 已解决 |

这本身即一项**文档可信度风险（P1 级）**：`docs/audit/` 多处结论与代码漂移，建议在报告签发时统一标注失效。

---

## 六、建议处置顺序

1. **立即（P1，低成本高收益）**：B1 指标内存无界 → 修；F1/F2/F3/F5 文档纠偏（可直接消除 CI 死链门禁风险）
2. **本迭代（P2 代码）**：A2 ClamAV 路径透传、A1 delayed EDU 落库、A4 事务原子性、B2/B3 查询优化
3. **安全加固（P2 配置）**：C2/C3 改为 fail-closed 或明确告警、C4 移除弱默认、C1 key_rotation 走完整授权
4. **收尾（P3 + 文档）**：D 系列 SDK 对齐、E 系列债务、F4/F6/F7/F8 清理

---

## 七、E 系列债务核验与处置记录（2026-10-04 收尾批）

> 本节为 §四 E 表的落地结论，均已对照代码与 lint 配置取证。

| 编号 | 报告原述 | 核验结论 | 处置 |
|------|----------|----------|------|
| **E1** | `#[allow(clippy::)]` ≈160 处（含 61 处 `too_many_arguments`） | 前提不成立：绝大多数 `#[allow]` 在 `-D warnings` 门下为 **load-bearing**（删除即门禁失败），其治理需重构（参数结构体化 / 类型别名），部分落在非本批文件内。仅 2 处 `clippy::panic` allow 完全冗余（根包及 `synapse-web`/`synapse-test-utils` 的 `[lints.clippy]` 已置 `panic = "allow"`；6 个成员 crate 覆盖为 `deny`，其 `panic` allow 属 load-bearing）。`needless_raw_string_hashes`(66) 全部位于 `target/` 构建产物，非源码。 | 删除 2 处完全冗余属性（`src/bin/synapse_ledger_export.rs`、`src/bin/synapse_worker.rs`）；其余 load-bearing 项转独立重构批次 |
| **E2** | `create_event` ≈194 行超长函数 | 属实 | 已拆分（主函数 + 3 个私有辅助 `normalize_redaction_placement`/`persist_event`/`run_post_create_side_effects`，行为等价） |
| **E3** | `src/e2ee/`+`src/cache/` 多层 facade 冗余 | 属实（根 crate 为纯 re-export 薄壳） | 已折叠为直接 re-export（删除 11 个薄壳 + `src/cache/mod.rs`，改写 `src/e2ee/mod.rs`、`src/lib.rs`） |
| **E4** | `vendor/pastey` fork 不受门禁 | 属实但 **by-design**：`RUSTSEC-2024-0436` 缓解，`[patch.crates-io]` 将停维护 `paste` 重定向至 pastey 0.2.3 逐字拷贝，`workspace.exclude` 排除、自带 `[lints.rust]`，源码零改动 | 归档，不处置 |
| **E5** | 债务标记 11 + 未实现 13 + unwrap/expect 887（多为测试） | 表述不准：`TODO/FIXME` 实际 6 处，全为非可执行项（1 处 vendored、1 处上游拷贝注释、4 处已解决 review TODO 的说明性 prose）；`todo!/unimplemented!` 全部位于 `#[cfg(test)]` 测试 mock 与 vendored 代码，生产实质缺口 0 | 归档，不处置 |
| **E6** | `.clippy.toml` 阈值放宽（cognitive-complexity=25 等） | 表述不准：仅 `too-many-lines-threshold=500` 相对默认(100)放宽；`cognitive-complexity=25`、`single-char-binding=4`、`too-many-arguments=7` 均为 clippy 默认值；`type-complexity=200` 相对默认(250)反而**收紧** | 归档，不处置 |
