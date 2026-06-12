# Changelog

> 所有项目的显著变更都会记录在此文件。
>
> 格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
> 版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。
>
> **当前基线**: v10.0.0（2026-06-12；迁移 v10 双文件基线，审计报告 v7.1）

---

## 版本速查

| 版本 | 发布日期 | 性质 | 主要内容 |
|------|----------|------|----------|
| [Unreleased](#unreleased) | TBD | 进行中 | C-5 Phase 3/4 vodozemac 互操作矩阵 + 清理自研路径 |
| [v10.0.0](#v10000---2026-06-12) | 2026-06-12 | 🚧 **当前基线** | P0/P1 全部修复 / v10 迁移基线 / clippy 门禁修复 / 文档同步 / P2 #7 L1 unwrap 治理 |
| [v8.0.0](#v8000---2026-06-06) | 2026-06-06 | 历史 | P0 全部修复 / v8 迁移基线 / E2EE vodozemac 收敛 Phase 1+2 / Step 10-12 工程门禁 |
| [v7.x](#v7x---2026-05-28-前) | 2026-05-28 前 | 历史 | 旧 `Cargo.toml` 版本基线，包含 v7 迁移文件（已被 v8 收敛） |

---

## [Unreleased]

> 此节列出已合入主分支但尚未发布的变更。
> 下一个版本号会基于此节归并。

### Added（新增）
- **C-5 Phase 3（E2EE 互操作）**：本地 vodozemac 互操作测试矩阵 19 个 case
  （Olm 账户/会话/线路编码 + Megolm 共享/monotonicity/前向保密 + pickle 兼容
  + `m.room_key` to-device payload + 算法拒绝），见
  [`src/e2ee/vodozemac_interop_tests.rs`](./src/e2ee/vodozemac_interop_tests.rs)。
  全部需 `E2EE_INTEROP=1` 显式启用。
- **Step 10（工程门禁）**：`scripts/ci/supply_chain_gate.sh` 接入 CI 主流程，
  阻断 `cargo-deny` + `cargo-audit` 任意违规。
- **Step 12（文档治理）**：[`docs/INDEX.md`](./docs/INDEX.md) 文档导航中枢，
  区分 `archive/` 与现行，纳入 PR 门禁。
- 审计可观测性：Megolm 互操作 metrics 7 个 + 记录方法 3 个
  （`megolm_vodozemac_pickle_persist_total` 等）。

### Changed
- **C-5 Phase 4 协议层包装边界进一步冻结**（2026-06-11）：
  - 在 `src/e2ee/crypto/` 与 `synapse-e2ee/src/crypto/` 两棵树中同步完成接口可见性收窄
  - `aes.rs`：删除桥接方法 `Aes256GcmKey::as_bytes`、`Aes256GcmNonce::as_bytes`、`Aes256GcmCipher::new`；将 `Aes256GcmNonce::{generate, from_bytes}` 与 `Aes256GcmCipher::{encrypt, decrypt}` 收为模块私有；新增 `Aes256GcmCipher::split_encrypted_data` 私有辅助方法聚合测试逻辑
  - `ed25519.rs`：新增 `Ed25519PublicKey::verify` 公开方法，封装签名验证；将 `Ed25519PublicKey::from_bytes` 收为模块私有，并继续删除 `Ed25519PublicKey::as_bytes` 与 `Ed25519KeyPair::verify` 测试桥接
  - 移除 `src/e2ee/mod.rs` 与 `synapse-e2ee/src/lib.rs` 的顶层 re-export，减少公开暴露面
  - 将 `src/e2ee/crypto/mod.rs` 与 `synapse-e2ee/src/crypto/mod.rs` 的子模块收为私有
  - 同步更新上层调用点 `signed_json.rs`，并移除对 `ed25519_dalek::{Verifier, VerifyingKey}` 的直接依赖
- **C-5 Phase 3 跨端验收入口整理**（2026-06-11）：
  - `scripts/test/run_sdk_verification_real_backend.sh` 新增 `SKIP_SDK_TEST=1` 与 `SDK_INTEROP_ARTIFACT_DIR`，可仅启动 live backend 并自动落基础证据
  - `docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md` 新增 Android/iOS 手动验收入口、Element Web 叠加方式、`artifacts/e2ee-interop/mobile/<run-id>/` 结果记录规范，以及 I-1 ~ I-8 的逐项执行 checklist
- E2EE 评估口径：审查报告 P0 项 4 状态从「🚧 Phase 1+2 ✅ / Phase 3 进行中 / Phase 4 待 Phase 3 收尾」更新为「🚧 Phase 1+2 ✅ / Phase 3 浏览器验证 ✅ / Phase 4 协议层边界基本冻结」。
- `ServiceContainer` 状态口径：核心字段数 35 → 48（实际，比 2.2 报告多 13）。
- `config/mod.rs` 行数：4081 → 4056（聚合文件，已拆 18 子模块）。

### Fixed
- 文档偏差：审查报告原 `Step 10 ❌ 未开始 0%` 与代码现状不符
  （`deny.toml` / `cargo-audit.toml` / `mutation-testing.yml` / `.tarpaulin.toml`
  均已就位）→ 已修正为 `✅ 已完成 95%`。

### Security
- 无新增。

### Deprecated
- 无新增。

### Removed
- 无新增（C-5 Phase 4 当前以边界冻结与跨端验收为主，自研 crypto/olm 的进一步删除需待跨端矩阵全绿后再评估）。

---

## [v8.0.0] - 2026-06-06

> 🚧 **预发布基线**。CI 全绿、`cargo clippy -D warnings` 0 错误 0 警告、
> `cargo check --all-features --locked` 0 错误。
> 关联基线：[`COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`](./docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md)。

### Added（新增）

#### P0 安全/正确性（9 项 C-* 全部完成 / C-5 进入收敛期）
- **C-1 联邦 X-Matrix 时间戳新鲜度校验**
  - 新增 `src/common/nonce_cache.rs::FederationNonceCache`（moka，TTL=60s，容量=1M）
  - 新增 `src/web/middleware/federation_auth.rs::verify_freshness`（±30s 滑动窗口）
- **C-2 Canonical JSON 字符转义**
  - `src/e2ee/signed_json.rs::escape_canonical_string` 处理 U+2028/2029/FFFD
- **C-3 Sync since token 单次解析**
  - `src/services/sync_service/mod.rs`：`since_token` 单次解析贯穿 `sync_with_request`
  - 同一 `Option<SyncToken>` 贯穿 `delete_messages_up_to` 和 `is_incremental`
- **C-4 CI 路由分层门禁**
  - 新增 `scripts/quality/check_route_layering.sh`
  - 集成到 Makefile 和 CI 流程
- **C-5 E2EE vodozemac 收敛**（Phase 1 ✅ + Phase 2 ✅ + Phase 3 🚧）
  - **Phase 1（2026-06-05）**：`MegolmProvider` 双路径抽象（`MegolmBackend::{Legacy, Vodozemac}`）
  - **Phase 2（2026-06-06）**：
    - 新增 `PickleFormat::{Legacy, Vodozemac, Dual}` 枚举
    - 新增 `megolm_sessions.pickle_format` / `vodozemac_pickle` 列（迁移文件
      `migrations/20260605120000_megolm_vodozemac_dual_write_v8.sql`）
    - 懒迁移 API：`promote_to_dual` / `list_legacy_sessions` / `count_by_pickle_format`
    - 7 个新 metrics + 3 个记录方法
  - **Phase 3（2026-06-06 启动）**：本地 vodozemac 互操作 19 个 case 已就位
- **C-6 JWT 旧 token 默认放行修复**
  - `src/auth/token.rs::is_legacy_token_window_open` 默认返回 `false`
- **C-7 TOTP 恒时比较**
  - `src/web/utils/admin_auth.rs`：TOTP 改用 `subtle::ConstantTimeEq::ct_eq`
- **C-8/C-9 迁移文件 Schema 冲突**
  - v8 统一基线（`00000000_unified_schema_v8.sql` + `00000001_extensions_v8.sql`）
  - 迁移目录从 50 个文件收敛至 4 个
  - 17 处 `_ts`/`_at` 后缀统一
  - 8 处 `#[sqlx(rename)]` 桥接消除
- **C-10 SAML 模块 `NOW()` 残留**
  - 修复 `src/services/saml_service.rs:332, 580, 778` 共 3 处
  - 改为 `EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`

#### P1 架构/质量（11 项全部完成）
- **M-1 ServiceContainer 分层**
  - 拆为 4 个子结构体（`E2ee` / `RoomSync` / `Federation` / `Admin`）
  - 48 核心字段（实际，2.2 报告为 35）
  - 文件行数 1201（vs 2.2 报告的 1408）
- **M-2 `common/config/mod.rs` 拆分**
  - 拆为 18 子模块（`auth` / `builtin_oidc` / `database` / `error` / `experimental` /
    `federation` / `identity` / `logging` / `manager` / `performance` / `push` /
    `rate_limit` / `retention` / `search` / `security` / `server` / `smtp` /
    `translate` / `voip` / `worker`）
  - 聚合文件 1976 行
- **M-3 `sqlx::query!` 全量迁移**（**已搁置**）
  - 替代方案：CI 强制 `schema_health_check` 门禁
  - 详见 `docs/synapse-rust/M3_PROGRESS.md`（**stale**）与
    `scripts/ci_schema_health_check.sh`
- **M-4 路由层强制使用 service**
  - `scripts/ci/check_route_storage_boundary.sh` 门禁部署
  - `scripts/quality/check_route_layering.sh` 配套
- **M-5 N+1/无限流硬性 `LIMIT`**
  - `src/storage/membership.rs::get_room_members` / `get_shared_room_users` 添加 `LIMIT 200`
  - `src/storage/event.rs::get_room_events_by_type` / `get_sender_events` 添加 `limit.min(200)`
  - `src/storage/room.rs::get_rooms_batch` 输入数组 `take(200)` 上限
- **M-6 联邦签名缓存 key 失效广播**
  - `KeyRotationManager`（`src/federation/key_rotation.rs`）
  - `FederationSignatureCache`（`src/cache/federation_signature_cache.rs`）
- **M-7 Typing/Presence 强制 Redis**
  - `CacheManager` L1+L2（`src/cache/mod.rs`）
- **M-8/M-9 `ApiError` 结构化 + TraceContext 透传**
  - `src/common/error.rs` 结构化日志（`tracing::error!(errcode, error, context)` 模式）
  - 7 个关键方法添加 `#[instrument]`
  - `tracing` crate 启用 `attributes` feature
- **M-10 巨型文件拆分**
  - 8 个文件已全部拆分
  - 仅剩 `common/config/mod.rs` 聚合文件（1976 行）

#### 工程门禁与 CI（Step 10）
- **`deny.toml`**：`cargo-deny` 配置（advisories/bans/licenses/sources）
- **`cargo-audit.toml` + `audit.toml`**：`cargo-audit` 阻断执行
  （`--deny warnings --deny unsound --deny yanked`）
- **`scripts/ci/supply_chain_gate.sh`**：Step 10 主门禁，集成
  `cargo-deny check` + `cargo-audit`；CI 中 `ci.yml:93, 318` 已在两个 job 中调用
- **`.github/workflows/mutation-testing.yml`**：cargo-mutants nightly
  （非阻塞，timeout 120min）
- **`.tarpaulin.toml`**：覆盖率门槛 `range = "70..90"`
- **`docs/INDEX.md`**：文档治理中枢，区分 `archive/` 与现行，纳入 PR 门禁

#### 测试整改（Step 8）
- 删除 `error.rs` 中 4 个套套逻辑测试，共 ~200 行
- 删除 `benches/` 中 7 个无 IO 伪性能测试，共 ~400 行
- 引入 `cargo-mutants` 接入 CI（`.github/workflows/mutation-testing.yml`）
- `tarpaulin.toml` 覆盖率门槛 `fail-under = 70`

#### 性能与可观测性（Step 9）
- `ApiError` 100% 结构化
- 7 个关键方法添加 `#[instrument]`
- `LIMIT 200` 在 3 个查询路径落地

#### 文档与发布基线（Step 12）
- `docs/synapse-rust/API_COVERAGE_REPORT.md`（vs Synapse v1.149.1）
- `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`（Matrix v1.18 + Synapse v1.153）
- `docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md`（C-5 完整迁移设计 + 状态报告）
- `docs/INDEX.md`（**新增**，2026-06-06）

### Changed
- 评估口径：审计报告 v2.4 → v2.5（2026-06-06）
- Matrix spec baseline: v1.13 → v1.18
- Synapse behavioral reference: v1.149.1 → v1.153.0

### Fixed
- DB Schema 漂移风险：`schema_health_check` CI 门禁
- 联邦 nonce 重放：✅ C-1
- Canonical JSON 跨语言兼容性：✅ C-2
- Sync token 解析竞态：✅ C-3
- TOTP 计时侧信道：✅ C-7
- SAML `NOW()` 运行时错误：✅ C-10
- ApiError 内部错误泄漏（`internal_with_log` / `database_with_log`）：~1200 处
  跨 119 个文件
- N+1 查询：M-5 落地
- 路由层直查 DB：M-4 门禁部署
- format drift：`.github/workflows/format-drift-tracking.yml` + `format-governance.yml`

### Security
- `cargo-deny` + `cargo-audit` 阻断执行
- 已豁免 2 条 RUSTSEC（均带 Review-by 期限）：
  - **RUSTSEC-2023-0071**（rsa 0.9.10 Marvin Attack）：仅用于 OIDC 签名，
    无解密路径；目标 2026-06-30 迁 ES256
  - **RUSTSEC-2024-0436**（paste 1.0.15 unmaintained）：传递依赖，
    仅编译期宏使用

### Deprecated
- 自研 Olm/Megolm crypto（`src/e2ee/olm/session.rs` /
  `src/e2ee/crypto/{aes,x25519}.rs`）— 已进入 C-5 Phase 4 边界冻结阶段，进一步删除需待跨端矩阵全绿后评估
  （**必须 Phase 3 跨 Element 客户端矩阵全绿**）

### Removed
- 50 个迁移文件 → 收敛至 4 个（`migrations/00000000_unified_schema_v8.sql` /
  `migrations/00000001_extensions_v8.sql` / `migrations/extension_map.conf` /
  `migrations/README.md`）
- 12+ 重复索引定义
- 17 处 `expires_ts` / `consumed_ts` / `logout_sent_ts` 命名（→ `_at` 后缀）

---

## [v7.x] - 2026-05-28 前

> 历史版本，迁移文件已并入 v8.0.0。保留作历史溯源。

### 已知问题（迁移至 v8.0.0 修复）
- 迁移文件 v7 Schema 冲突（`voice_usage_stats` 三重定义等）
- 30+ 内部表定义重复
- 69+ 跨文件表定义重复
- 18 张冗余表（已在 Batch-03 DROP）
- 63 处 `SELECT *` 脆弱查询
- 5+ 处布尔字段缺 `is_` 前缀
- 3 处 `NOW()` 赋值 BIGINT 列（**C-10**）

### Deprecated
- 旧基线 `00000000_unified_schema_v7.sql` → 已被 v8 替代

---

## 变更分类约定

每条变更归入以下类别之一：

- **Added**：新增功能、API、迁移、文档
- **Changed**：既有功能的行为/接口变更
- **Fixed**：Bug 修复、性能退化修复
- **Security**：CVE 修复、安全策略变更
- **Deprecated**：即将移除（保留过渡期）
- **Removed**：本版本已移除

跨多个类别的变更按"对用户最显著的类别"归类，
次要类别在条目末尾以"（另：...）"标注。

---

## 版本号约定

- **MAJOR**（如 v8 → v9）：数据库 schema 大版本升级、协议不兼容变更
- **MINOR**（如 v8.0 → v8.1）：新 API、新 MSC、稳定功能新增
- **PATCH**（如 v8.0.0 → v8.0.1）：Bug 修复、安全补丁、性能优化

预发布标签：`-alpha.N` / `-beta.N` / `-rc.N`（如 `v8.0.0-beta.1`）

---

## 与其他文档的关系

| 文档 | 用途 |
|------|------|
| [`docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`](./docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md) | 全面审查报告（本 changelog 的依据） |
| [`docs/INDEX.md`](./docs/INDEX.md) | 文档导航中枢 |
| [`README.md`](./README.md) | 项目门面 |
| `Cargo.toml` | 当前版本号机器源 |
| `git tag` | 实际发版 tag 序列 |
