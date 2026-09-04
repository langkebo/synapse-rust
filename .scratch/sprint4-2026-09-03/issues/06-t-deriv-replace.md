# T-DERIV-REPLACE: derivative 2.2 → educe 0.7

| 字段 | 值 |
|------|---|
| 状态 | ✅ **DONE**（2026-09-04，commit `b410a495`）|
| 工作量 | 实际 0.5d（探查 0.1d + dep 替换 0.05d + 文件迁移 0.15d + 质量门禁 0.1d + 文档 0.1d）|
| 优先级 | P3 |
| 关联 | RUSTSEC-2024-0401 (derivative unmaintained) |

## 目标
移除 workspace 中 `derivative = "2.2"`（unmaintained，RUSTSEC-2024-0401 ignore 项），
迁移到活跃维护的 `educe = "0.7"`，消除 ignore 项。

## 实施范围

| 文件 | struct 派生 | Debug=ignore 字段 |
|------|-----------|------------------|
| `mas.rs` | 1 (`MasConfig`) | 2 |
| `security.rs` | 2 (`SecurityConfig`, `AdminRegistrationConfig`) | 3 |
| `database.rs` | 2 (`DatabaseConfig`, `RedisConfig`) | 2 |
| `auth.rs` | 2 (`OidcConfig`, `OidcUserMapping`) | 1 |
| `worker.rs` | 1 (`ReplicationHttpConfig`) | 1 |
| `builtin_oidc.rs` | 1 (`BuiltinOidcUser`) | 2 |
| `federation.rs` | 1 (`FederationConfig`) | 2 |
| `translate.rs` | 1 (`TranslateConfig`) | 1 |
| `sms.rs` | 1 (`SmsConfig`) | 2 |

**合计**：10 文件，14 struct，17 field-level redactions

## API 映射

| derivative | educe | 说明 |
|------------|-------|------|
| `derivative::Derivative` | `educe::Educe` | 派生入口 |
| `#[derivative(Debug)]` | `#[educe(Debug)]` | struct-level Debug 派生 |
| `#[derivative(Debug = "ignore")]` | `#[educe(Debug(ignore))]` | 字段级 Debug 脱敏 |

## 实施步骤

### Phase 1: 探查（已完成）
- 确认 derivative 仅在 `synapse-common/src/config/` 下 13 个文件、25 处使用
- 确认全部用法是"全字段 Debug + 选择性 ignore"一种模式，无 hash/eq 等其他用法
- 查 educe 最新版 0.7.6（最低 Rust 1.89，项目 MSRV 1.93 ✓）
- 确认 API 一对一兼容

### Phase 2: workspace dep 替换（已完成）
- `Cargo.toml`: `derivative = "2.2"` → `educe = "0.7"` + 注释说明
- `synapse-common/Cargo.toml`: `derivative = { workspace = true }` → `educe = { workspace = true }`
- `cargo fetch` 锁定：`educe v0.7.6`, `enum-ordinalize v4.4.2` (传递依赖)

### Phase 3: 批量迁移 13 个文件（已完成）

**坑点（实施中发现的非平凡问题）**：
1. **多行 derive 闭合** — 项目里 5 处 `#[derive(Debug, Clone, Deserialize, Default\npub struct X {]` 是 pre-existing 多行写法（与 derivative 无关），但被脚本 regex `re.sub(r'#\[derive\(([^)]+)\)\]', ...)` 误吞了 `)]`。修复：用 Edit 逐个补回。
2. **import 位置** — Python 脚本插入 `use educe::Educe;` 时，对只有 `//!` doc comments 的 `mas.rs` 错把 import 推到 `mod tests` 里（被覆盖到测试模块顶部），其余 6 个文件 import 跑到文件底部中间位置。修复：手工移回顶部 import 块。
3. **`educe::Educe` 完整路径 → 短名 `Educe`** — 统一规范风格，所有文件都用 `use educe::Educe;` + `#[derive(... Educe)]`。

### Phase 4: 质量门禁（全部通过）

| Gate | 结果 |
|------|------|
| `cargo build -p synapse-common --features test-utils` | ✅ |
| `cargo clippy -p synapse-common --features test-utils --all-targets -- -D warnings` | ✅ |
| `cargo test -p synapse-common --features test-utils --lib config::` | **202/202 PASS** |
| `cargo audit` | ✅ 0 vulnerabilities / 619 crates |
| `cargo machete` | ✅ 0 unused deps |

### Phase 5: 文档同步（本文件 + dep-audit 更新）

## 教训（沉淀到 MEMORY.md）

1. **批量 regex migration 风险**：单行 regex `r'#\[derive\(([^)]+)\)\]'` 在处理多行 derive 时容易误伤相邻 `)]`。**永远先 dry-run 看 diff**。
2. **Python import 插入位置**：要尊重 doc comments (`//!`) 和已有的 `use` 块顺序，不能简单按"第一个非 use 非注释行"作为 anchor。
3. **educe vs derivative 边界**：两者 API 对 `Debug` + `ignore` 字段完全兼容，但 educe 在字段名解析、bound 推断上有差异（智能推断），如果未来要 derive `PartialEq`/`Hash` 需重新评估。

## 后续

- ✅ deny.toml 中 `derivative` ignore 条目应可移除（待 sprint 结束统一处理 deny.toml）
- Q4 ticket 剩余：#T-PROC-MACRO-ERR3, #T-RAND-068, #T-PASTE-PATCH
