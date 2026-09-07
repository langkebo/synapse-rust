# synapse-rust 优化实施报告

> 实施时间：2026-09-07
> 基于 `artifacts/synapse-rust-architecture-audit-2026-09-07.md`

---

## 已完成优化

### ✅ 1. 移除零逻辑透传壳——`user_lock_service.rs`（P2）

- **问题**：`synapse-services/src/user_lock_service.rs`（107 行）每个方法都是 `self.user_store.xxx().map_err(ApiError::internal_with_context)` 转发，零业务逻辑。且从未被 wiring 层实例化——不是死代码而是死结构。
- **操作**：删除整个文件；从 `synapse-services/src/lib.rs` 移除 `pub mod user_lock_service`；`account/mod.rs:26` 的 `pub use crate::user_lock_service::*` 保留（无副作用，glob re-export 空模块）。
- **验证**：`cargo check --workspace --features "test-utils,burn-after-read"` 通过。

### ✅ 2. 修复 clippy `doc list item without indentation`（P1）

- **问题**：`server.rs`（5 处）+ `rate_limit_config.rs`（1 处）的 `/// \`xxx\` field.` 占位符 doc comment 紧接在 `#[serde(default)]` attribute 后，rustdoc 把它们解析成缩进错误的列表项，触发 `clippy::doc_markdown` 编译错误（因 `-D warnings`）。
- **操作**：删除 6 处冗余的 `/// \`xxx\` field.` 占位符（上方已有完整中文 doc 块）。
- **验证**：`cargo clippy --workspace --lib --features "test-utils,burn-after-read" -- -D warnings` 通过（0 errors）。

### ✅ 3. 批量清理连续重复 `/// See [xxx].` doc 注释（P2）

- **问题**：audit 识别出 1,172 处 `See [` 注释，其中大量是连续重复的（AI 生成副产品）。
- **操作**：用 Python 脚本精确移除**连续重复**的 doc comment 行（仅删连续相同行，保留第一条）。
- **结果**：1,880 行冗余 doc 注释在 265 个文件中移除：
  - `synapse-services/src/`：1,025 行（136 文件）
  - `src/web/` + `synapse-storage/src/` + `synapse-federation/src/` + `synapse-common/src/` + `synapse-cache/src/` + `synapse-e2ee/src/`：855 行（129 文件）
- **验证**：`cargo build --workspace --features "test-utils,burn-after-read"` 通过。

---

## 验证状态

| 检查项 | 结果 |
|---|---|
| `cargo build --workspace --features "test-utils,burn-after-read"` | ✅ 通过，0 warnings |
| `cargo clippy --workspace --lib --features "test-utils,burn-after-read" -- -D warnings` | ✅ 通过，0 errors |
| `cargo check --workspace --features "test-utils,burn-after-read"` | ✅ 通过 |
| `user_lock_service` 不再存在 | ✅ 文件已删 |
| doc list 错误归零 | ✅ 6 处占位符已删 |

---

## 后续可处理（不在本次范围，留作 backlog）

| 优先级 | 问题 | 建议行动 |
|---|---|---|
| P0-1 | L1 缓存全局排他锁（`deadlines` map，`synapse-cache/src/lib.rs`） | 删除 `deadlines`，改用 moka 原生 `insert_with_ttl` |
| P0-2 | 巨型单文件（`friend_room_service/mod.rs` 3022 行含生产+测试+bench） | 拆出 `bench/` 和 `tests/` 子模块 |
| P0-3 | `transaction.rs` 662 行单函数 | 拆为签名校验/持久化/广播 3 个子 async fn |
| P1-1 | Federation `m.receipt` EDU 缺失 | 新增 EDU 变体 + 分发 + 出站触发 |
| P1-2 | E2EE 核心模块零单元测试 | 按 `key_request → device_trust → secure_backup` 优先级补测 |
| P1-3 | 请求体全面加 `#[serde(deny_unknown_fields)]` | 脚本扫描所有 `*Request` struct |
| P1-4 | Federation SSRF 防护 | HTTP client 注入 private IP 黑名单 Connector |
| P1-5 | `/sync` 热路径每次打未缓存 `get_joined_rooms` | 复用缓存 joined-rooms 列表 |
| P1-6 | 清理 240 处 `dead_code` allow（audit 数） | 枚举非 test 上下文，逐个删或重构 |
| P2-1 | `synapse-web` 空壳 vs `src/web` 未迁出 | 二选一：删除空壳 或 迁入 |
| P2-2 | 全局 `in_flight` 单飞锁改 moka 原生 single-flight | 替换为 `cache.get_with(key, async { ... })` |
| P2-3 | 收敛测试基础设施（`src/test_utils.rs` 1285 行与 crate 内 helper 重复） | 跨 crate 共享测试 helper 模块 |
