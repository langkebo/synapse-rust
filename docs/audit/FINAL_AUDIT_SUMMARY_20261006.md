# synapse-rust 遗留问题审计 —— 最终总结报告

> 项目：synapse-rust v6.2.0（Rust Matrix homeserver，Cargo workspace，~10 万行）
> 报告日期：2026-10-06
> 基准：Synapse v1.161.0 / Matrix Spec v1.15（v12 房间版本）
> 性质：**结项总结**（单页收敛）。逐条明细、证据与复现命令见权威报告 [`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md)，本文件不重复其内容。

---

## 一、最终结论

本轮针对 7 个领域的全面遗留问题排查**已全部结项**：

- **未发现未修复的 P0**；上一版登记的安全 P0（`login_as_user` 越权）经复核**已修复**。
- **3 个 P1 全部闭环**（联邦 `/send_join` 缺 `event` 字段、两份权威文档失实）。
- **26 个 P2 全部闭环**（末项 `COMPAT-05` 复核闭环）；另列依赖卫生 `DEAD-01` 亦已闭环。
- **27 个 P3 全部处置**（21 闭环 + 6 正式登记为 by-design）。
- **9 条历史结论被复核推翻**（证伪 / 已修复），已从权威清单移除或降级。
- **既有 `--all-targets` 编译错误查明为非缺陷**（缺 `--features test-utils` 的调用假阳性，见 §四）。

即：**报告 §2 中所有编号均有终态；无未决缺陷，无待办修复项。**

---

## 二、发现总览（去重后）

| 优先级 | 数量 | 终态 |
|:--:|:--:|------|
| P0 | 0 | 无（上一版 P0 已修复） |
| P1 | 3 | 全部闭环（`FED-01`、`DOC-01`、`DOC-02`） |
| P2 | 26 | 全部闭环（含复核闭环 `COMPAT-05`）；另列 `DEAD-01` 亦闭环 |
| P3 | 27 | 21 闭环 + 6 by-design 登记 |
| 证伪/已修复 | 9 | 已从权威清单移除或降级 |

---

## 三、基线门禁终态（全绿）

| 门禁 / 工具 | 结果 |
|-------------|------|
| `cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` | PASS（exit 0，无告警） |
| `cargo check --workspace --all-targets --features test-utils --locked` | PASS（EXIT=0，0 error / 0 warning） |
| `cargo deny check advisories` | PASS（advisories ok） |
| `cargo machete` | PASS（0 个未使用依赖） |
| `./scripts/check_fmt_ratchet.sh` | PASS（current=0 / baseline=0） |
| `python3 scripts/ci/check_clippy_allow_ratchet.py` | PASS（163 unjustified / 98 文件，OK） |
| `bash scripts/ci/check_sqlx_cache_fresh.sh --static` | PASS（1357/1357 命中；缓存 1324 条查询元数据） |
| `cargo test --test unit --features test-utils doc_credibility_guard` | PASS（10 passed） |
| `cargo test -p synapse-e2ee --lib cross_signing` | PASS（44 passed，含新增 4 单测） |
| 契约/分层/预算/ratchet 系列（route contract、web layering、connection/memory budget、trait/rand/ts-order） | PASS（见权威报告 §1.2） |

### 3.1 全量测试与性能门禁运行证据（2026-10-06）

> 承接上方静态门禁：本轮执行**全部四条 CI 测试车道 + 三个性能门禁**，全部 PASS，补齐运行证据（明细与复现口径见权威报告 §1.5）。

| 车道 / 门禁 | 结果 |
|-------------|------|
| lib（`cargo test --workspace --lib --all-features`） | ✅ **6683 passed / 0 failed**（cache 94 / common 996 / e2ee 394 / federation 308 / root 40 / services 2184 / storage 1851 / test-utils 4 / web 812） |
| unit（`nextest --test unit`） | ✅ **1824 passed / 0 failed / 2 skipped** |
| integration（`nextest --test integration`） | ✅ **1534 passed / 0 failed / 0 ignored**（4121.50s） |
| e2e（`nextest --test e2e`） | ✅ **20 passed / 0 failed / 7 ignored**（ignored 为需运行中 homeserver 的 opt-in 用例） |
| 计算性能门禁 | ✅ **PASSED**（17 项测量 / 0 越界） |
| Sliding Sync 性能门禁 | ✅ **PASSED**（33/33 样本 < 5000ms；p95 ≈ 0.70–0.90ms） |
| 分页性能门禁 | ✅ **PASSED**（keyset 较 offset **10.54×**） |

> 运行中修复 1 处集成**存量红**：`api_auth_routes_tests::test_versions_and_public_capabilities_match_declared_room_version_surface`，判定为**测试过时**（根因 G-1：`7489b247f` 创建面收窄至 v12、`8687d8335` 漏改本文件），已最小修复并复跑转绿——见权威报告 §8.13。

---

## 四、闭环批次一览（权威报告 §8）

| 批次 | 覆盖编号 | 摘要 |
|------|----------|------|
| §8.1 | `FED-01` | `/send_join` v1/v2 响应回显签名 join PDU（`event`）+ 联邦集成测试 |
| §8.2 | `DOC-01`、`DOC-02` | 重写 `project_rules.md` §1.3/§7.0；刷新权威问题清单 |
| §8.3 | `DEAD-01` | 删除 10 个未使用依赖；`cargo machete` 入 pre-push（阻断级） |
| §8.4 | `COMPAT-06` | `.sqlx` 新增「源码查询指纹 vs 缓存」新鲜度断言 |
| §8.5 | `PERF-04`、`PERF-05` | 预算门禁改读真实运行配置；`high` 风险改为阻断 |
| §8.6 | `COMPAT-01/02/03/04`、`PERF-02/03`、`DOC-03/04/05` | 协议/兼容/性能/文档批次闭环 |
| §8.7 | `CQ-04`、`RED-02/03/04`、`DEAD-02/03` | P3 卫生批次 |
| §8.8 | `CQ-01`、`RED-01`、`DOC-11` | P3 收尾批次（allow ratchet、去重 INSERT、文档门禁扩容） |
| §8.9 | `SEC-01`、`FED-02`、`PERF-01`、`PERF-06`、`CQ-02`、`CQ-03`、`CQ-05` | P2 剩余批次 |
| §8.10 | 文档卫生 6 项 + 代码质量/低风险安全 5 项 | P3 剩余批次（含 `SEC-02/03/06`、`PERF-08`、`CQ-06/08`、`DOC-06~10`） |
| §8.11 | `CQ-07`、`FN-03`、`FN-04` + 需决策项核实 + by-design 登记 | 剩余收尾批次（3 实施 + 6 复核闭环 + 6 by-design） |
| §8.12 | 编译错误排查 | `--all-targets` 缺 `test-utils` 的 312 假阳性，**非缺陷** |

**§8.12 关键结论**：`cargo check/clippy --all-targets` **不带** `--features test-utils` 时的 312 处 `E0282/E0432/E0433`，根因为 `test_mocks` 以 `#[cfg(any(test, feature = "test-utils"))]` 门控、跨 crate 构建 lib test 时需 feature 传播；带该 flag 即为 **0 error / 0 warning**。判为 **by-design，不改代码**（CI / preflight / `.cargo/config.toml` / `.rust-analyzer.toml` 已一致落实该 flag）。

---

## 五、残余 by-design 清单（有意保留，不修）

| 编号 | 保留理由 |
|------|----------|
| `COMPAT-07` | `.well-known` 逐端点复核无残留缺陷，复核即闭环 |
| `COMPAT-09` | 稳定 / unstable 双前缀并存，服务合并前发布的旧客户端 |
| `FN-05` | voice `/convert` `/optimize` `/transcription` 以 501 明确「未实现」 |
| `FN-06` | `verify_secure_backup_passphrase` 恒 410（旧流程已废弃） |
| `FN-07` | 联邦 legacy keys 返回 `M_UNRECOGNIZED`（规范允许） |
| `FN-08` | MSC 编号借用/形状漂移，已在 `MSC_SEMANTICS.md` 逐条登记 |

> 另：`FN-03` 修正后**签名缺失仍返回空串**（legacy 兼容），非「恒空」，属正确性修正后的既有兼容行为。

---

## 六、报告关系与后续

- **权威全面报告**：[`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md)（本总结的上游来源；取代已归档的 20261004 版）
- **权威问题清单**：[`UNRESOLVED_ISSUES_SUMMARY.md`](UNRESOLVED_ISSUES_SUMMARY.md)（当前仍存问题）
- **安全专项**：[`PERMISSION_RBAC_AUDIT_2026-10-05.md`](PERMISSION_RBAC_AUDIT_2026-10-05.md)（其 P0/P1 已复核为已修复）
- **索引**：[`INDEX.md`](INDEX.md)

**后续唯一持续项**：文档防漂移门禁仍覆盖过窄（`DOC-11` 已扩容，但无法完全自动拦截回归），需在后续新增/变更权威文档时维持人工核对。

---

**生成时间**：2026-10-06
**下次计划更新**：2026-10-13
