# CI 安全分级策略

本文档定义 synapse-rust CI 安全工具链的分级策略，明确哪些工具会阻断构建、哪些仅告警。

---

## 安全工具矩阵

| 工具 | 维度 | 级别 | 阻断条件 | 配置位置 |
|------|------|------|---------|---------|
| `cargo-deny` | 许可证 / 依赖 bans / 来源 | **Grade A (阻断)** | 许可证违规、未知 registry、wildcard 依赖、yanked crate | `deny.toml` |
| `cargo-audit` | CVE / 安全公告 | **Grade A (阻断)** | 高危/中危 CVE 未列入 ignore 清单 | `.cargo/audit.toml` |
| `cargo-geiger` | `unsafe` 代码检测 | **Grade A (阻断)** | 生产代码 `unsafe` 超过 baseline | `scripts/ci/run_cargo_geiger.py` + `geiger_baseline.json` |
| `cargo-outdated` | 依赖新鲜度 | **Grade B (警告)** | 安全相关 crate 有更新时告警 | CI 步骤 (`continue-on-error: true`) |
| `rand::rng()` 扫描 | 特定漏洞防护 | **Grade A (阻断)** | 代码中出现 `rand::rng()` 调用 | CI 步骤 (`git grep`) |

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
- **例外管理**：在 `deny.toml` 的 `ignore` 列表中记录，需带理由和 review-by 日期

#### cargo-audit
- **检查内容**：RustSec 安全公告数据库中的已知 CVE
- **阻断条件**：
  - 高危（critical/high）CVE 未列入 ignore 清单
  - 中危（medium）CVE 未评估即合并
- **例外管理**：在 `.cargo/audit.toml` 的 `ignore` 列表中记录，需带：
  - 漏洞编号（如 `RUSTSEC-2026-0097`）
  - 影响范围说明
  - 缓解措施
  - review-by 日期

#### cargo-geiger
- **实现**：`scripts/ci/run_cargo_geiger.py`（JSON 输出 + baseline ratchet）
- **检查内容**：Rust 代码中的 `unsafe` 块
- **阻断条件**：
  - 生产代码（非 `tests/` 目录文件）unsafe 总数超过 baseline
  - 新文件出现 unsafe（baseline 中未记录）
- **非阻断但追踪**：
  - 测试代码（`tests/` 目录）中的 `unsafe` 块数量需记录基线，新增时告警
- **当前基线**（截至 2026-07-23，`scripts/ci/geiger_baseline.json`）：
  - 生产文件 unsafe：4（全部在 `src/` 文件的 `#[test]` 函数中，cargo-geiger 按文件统计不区分 `#[test]`；用于 `std::env::set_var`/`remove_var`，Rust 2024 要求 unsafe）
  - 测试文件 unsafe：0
- **注意**：cargo-geiger 按文件级别报告，不区分同一文件中的 `#[test]` 函数和非测试代码。baseline 中的 4 个 "生产" unsafe 实际上是 `src/` 文件内 `#[test]` 函数的环境变量操作，未来可通过迁移到 test-only helper crate 来降低 baseline。

#### rand::rng() 扫描
- **检查内容**：代码中是否出现 `rand::rng()` 调用
- **阻断条件**：任何 `rand::rng()` 调用
- **背景**：防御 RUSTSEC-2026-0097（rand unsoundness with custom logger）

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

| 工具 | 例外编号 | 理由 | review-by |
|------|---------|------|-----------|
| cargo-audit | RUSTSEC-2023-0071 | rsa 仅用于签名，不涉及解密 | 2026-06-30 |
| cargo-audit | RUSTSEC-2024-0436 | paste 为编译时宏，无运行时风险 | 2026-06-30 |
| cargo-audit | RUSTSEC-2025-0123 | opentelemetry-jaeger 为非默认可选依赖 | 2026-06-30 |
| cargo-audit | RUSTSEC-2026-0097 | 项目使用 tracing_subscriber 而非自定义 logger | 2026-05-15 |
| cargo-audit | RUSTSEC-2026-0173 | proc-macro-error2 为编译时 proc-macro | 长期跟踪 |

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
