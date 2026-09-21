# CI 安全分级策略

本文档定义 synapse-rust CI 安全工具链的分级策略，明确哪些工具会阻断构建、哪些仅告警。

---

## 安全工具矩阵

| 工具 | 维度 | 级别 | 阻断条件 | 配置位置 |
|------|------|------|---------|---------|
| `cargo-deny` | 许可证 / 依赖 bans / 来源 | **Grade A (阻断)** | 许可证违规、未知 registry、wildcard 依赖、yanked crate | `deny.toml` |
| `cargo-audit` | CVE / 安全公告 | **Grade A (阻断)** | 高危/中危 CVE 未列入 ignore 清单 | `.cargo/audit.toml` |
| `cargo-geiger` | `unsafe` 代码检测 | **Grade A (阻断)** | production `unsafe` 单向棘轮（只许减少；减少须收紧基线）| `scripts/ci/run_cargo_geiger.py` + `geiger_baseline.json` |
| `cargo-outdated` | 依赖新鲜度 | **Grade B (警告)** | 安全相关 crate 有更新时告警 | CI 步骤 (`continue-on-error: true`) |
| `rand::rng()` 扫描 | 特定漏洞防护 | **Grade A (阻断)** | **新增** `rand::rng()` 调用（棘轮，baseline 47）| `scripts/ci/check_rand_rng_ratchet.sh` |

---

## 分级详解

### Grade A — 阻断级

这些工具在 CI 中失败时会**阻断 PR 合并**。

#### cargo-deny
- **检查内容**：许可证合规性、依赖版本唯一性、来源 registry 白名单
- **阻断条件**：
  - 使用了不允许的许可证
  - 依赖了未知 registry 或 git 来源
  - 存在 wildcard 版本依赖
  - yanked crate（仅 warn，但会升级为阻断）
- **例外管理**：在 `deny.toml` 的 `advisories.ignore` 列表中记录编号 + review-by 日期；
  **理由只写在 `.cargo/audit.toml`**（单一真相源），两份清单必须一致

#### cargo-audit
- **检查内容**：RustSec 安全公告数据库中的已知 CVE
- **阻断条件**：
  - 高危（critical/high）CVE 未列入 ignore 清单
  - 中危（medium）CVE 未评估即合并
- **例外管理**：在 `.cargo/audit.toml` 的 `ignore` 列表中记录，需带：
  - 漏洞编号（如 `RUSTSEC-2023-0071`）
  - 影响范围说明
  - 缓解措施
  - 复核证据（**能当场重跑的命令及其输出**）与 review-by 日期
  - 不再匹配任何依赖的条目：**删除**，不要续期（死条目会在降级时静默放行）

#### cargo-geiger
- **实现**：`scripts/ci/run_cargo_geiger.py`（同 feature 集扫两次：`cargo geiger` 与
  `cargo geiger --include-tests`，逐包相减得到 test-only）
- **检查内容**：workspace 成员（`id.source = {"Path": …}`）的 `unsafe` 表达式数量
- **阻断条件**（**两个单向棘轮**，2026-09-21 裁定）：
  - production unsafe **超过**基线 ⇒ 红（新增 unsafe 必须改掉，不许加基线）
  - production unsafe **低于**基线 ⇒ 也红（好事，但必须同步收紧基线，否则基线会腐烂成松上限）
  - test-only unsafe 超过基线 ⇒ 红
  - 解析不到、`packages` 形状变化、逐条清单之和与总数不一致、`review_by` 过期 ⇒ exit 2
- **当前基线**（`scripts/ci/geiger_baseline.json`，逐条列明理由 + `review_by`）：
  - production unsafe：**2**（`synapse-common` 1 = `test_schema_guard.rs` 的 `libc::atexit`
    退出兜底，该模块按设计无条件编译；`synapse-rust` 1 = 未定位，源码里没有任何 unsafe
    字面量，疑为宏展开）
  - test-only unsafe：**8**（`synapse-common` 2 + `synapse-services` 2 是手写的
    `set_var`/`remove_var`；`synapse-rust` 4 同为未定位的宏展开类）
- **注意**：cargo-geiger 的 JSON **不含文件路径**（0.13 起 `packages` 是 list、计数器嵌套），
  所以生产/测试的区分只能靠两次扫描相减，无法按文件分类；`geiger_baseline.json` 里每条
  production 记录都必须写明站点、理由与 `review_by`，且清单之和必须等于总数，否则门禁 exit 2。

#### rand::rng() 扫描
- **检查内容**：代码中 `rand::rng()` 调用的数量是否超过 baseline
- **阻断条件**：**新增**调用（棘轮：`current > baseline` 红；`current < baseline` 也红，要求收紧）
- **实现**：`scripts/ci/check_rand_rng_ratchet.sh` + `scripts/ci/rand_rng_baseline`
- **背景**：防御 RUSTSEC-2026-0097（rand unsoundness with custom logger）
- **2026-09-21 复核**：advisory 的 `patched` 区间为 `>= 0.10.1` / `>= 0.9.3, < 0.10.0` /
  `>= 0.8.6, < 0.9.0`，而 `Cargo.lock` 是 rand **0.8.7** 与 **0.9.5** —— 都在已修复区间内，
  所以 `.cargo/audit.toml` 的 `RUSTSEC-2026-0097` ignore 已**删除**（它在依赖降级回受影响
  版本时会静默放行，去掉它反而让 cargo-audit 自己把关）。本扫描作为**纵深防御**保留。
- 原实现是绝对禁令，而树上有 47 处存量 ⇒ 永远不可能绿（§14.14.2）
- **Review-by 2026-12-21**

---

### Grade B — 警告/追踪级

这些工具在 CI 中失败时**不会阻断 PR**，但会生成告警报告供 review。

#### cargo-outdated
- **检查内容**：依赖是否有新版本可用
- **告警条件**：安全相关 crate（加密、TLS、HTTP 等）有更新时
- **追踪 crate 列表**：
  - `x25519-dalek`, `ed25519-dalek`, `curve25519-dalek`
  - `aes-gcm`, `chacha20poly1305`
  - `ring`, `openssl`, `rustls`
- **处理流程**：定期 review 告警 → 评估升级影响 → 排期升级

#### 测试代码 unsafe 追踪
- **检查内容**：`tests/` 目录下的 `unsafe` 块数量变化
- **告警条件**：新增测试 `unsafe` 块（不阻断）
- **基线管理**：当前 4 处，记录于本文档

---

### Grade C — 信息级

这些工具提供信息参考，不直接影响构建状态。

- **代码覆盖率报告**（`cargo-tarpaulin`）：覆盖率趋势参考
- **性能基准报告**（`cargo bench`）：性能趋势参考
- **文档质量检查**：文档完整性参考

---

## 例外管理流程

对于 Grade A 工具需要添加例外的情况：

1. **评估**：确认漏洞/问题是否真实影响项目
2. **记录**：在对应配置文件中添加 `ignore` 条目，包含：
   - 漏洞/问题编号
   - 影响范围说明
   - 缓解措施（如"仅用于签名，不涉及解密"）
   - review-by 日期
3. **审批**：需 maintainer 审批后方可合并
4. **定期 review**：在 review-by 日期前重新评估

### 当前例外清单

**单一真相源 = 配置文件本身**，不在这里再抄一份：

| 工具 | 例外清单所在 |
|------|-------------|
| cargo-audit | `.cargo/audit.toml` 的 `ignore`（含理由、复核证据、`Review-by`）—— **理由的单一真相源** |
| cargo-deny | `deny.toml` 的 `advisories.ignore`（只放 cargo-deny 真的会命中的编号 + `Review-by`） |

`deny.toml` 的清单必须是 `.cargo/audit.toml` 的**子集**（不要求相等：同一个编号在两个工具里
命中面可能不同，实测 `RUSTSEC-2024-0436`/paste 只在 cargo-audit 侧命中），且两份文件里每个
`Review-by` 都不得过期 —— 由
`tests/unit/ci_test_scope_tests.rs::advisory_review_dates_are_not_overdue` 把守。

本文档原来那张表**已删除**：它是同一职责的第三份副本，且已经漂移（列了配置里根本不存在的
`RUSTSEC-2025-0123`，又漏了配置里的 `RUSTSEC-2024-0388`）。2026-09-21 复核时另外删掉了三条
**不再匹配任何依赖**的 ignore：`RUSTSEC-2024-0388`（derivative）、`RUSTSEC-2026-0173`
（proc-macro-error2，现为 proc-macro-error3）、`RUSTSEC-2026-0097`（rand 0.8.7/0.9.5 已在
advisory 的 `patched` 区间内）。这批失效条目在
`docs/audit/PROJECT_ACTUAL_ISSUES_2026-09-14.md` M-6 里已被记录，但当时建议"三条全删"时没有
核对 cargo-audit 侧（paste 仍会被它命中），所以一直没修。

---

## 升级路径

当发现中危漏洞时的处理流程：

1. **确认**：复现漏洞影响范围
2. **评估**：是否可被利用、影响面多大
3. **决策**：
   - 立即修复 → 创建 PR
   - 列入例外 → 按例外管理流程记录
4. **跟踪**：设置 review-by 日期，到期前重新评估
5. **升级**：如果漏洞被利用或影响扩大，立即升级处理

---

## 相关文档

- `deny.toml` — cargo-deny 配置
- `.cargo/audit.toml` — cargo-audit 配置
- `docs/audit/07_security_audit.md` — 最新安全审计报告
- `.github/workflows/ci.yml` — CI 工作流定义
