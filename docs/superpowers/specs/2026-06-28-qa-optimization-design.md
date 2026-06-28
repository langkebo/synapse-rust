# QA Optimization Design — synapse-rust v6.0.4

**日期:** 2026-06-28
**分支:** main
**基线报告:** `/tmp/qa-baseline-2026-06-28.html`
**基线评分:** 76/100
**目标评分:** 94/100
**策略:** 方案 C 混合策略 — 单点问题最小 diff + 系统性问题结构性改进

---

## 问题清单

| ID | 问题 | 严重度 | 类型 | 阶段 |
|----|------|--------|------|------|
| B1 | 代码覆盖率 20.11%，远低于 70% 门禁 | Critical | 系统性 | Phase 3 |
| B2 | 集成测试需要 PostgreSQL 环境，本地无法运行 | Critical | 系统性 | Phase 1 |
| B3 | 性能基准编译失败 (target 目录损坏) | Critical | 单点 | Phase 1 |
| I1 | 3 个 Clippy 警告 | Low | 单点 | Phase 1 |
| I2 | Service 层错误全映射为 500 | Medium | 系统性 | Phase 2 |
| I3 | media.rs 远程获取错误被静默吞掉 | Medium | 单点 | Phase 2 |
| I4 | 管理注册 IP 检查逻辑分散 | Low | 系统性 | Phase 2 |
| I5 | r0/v1/v3 三版本路由重复 | Low | 系统性 | Phase 3 |
| I6 | 缺少本地 pre-commit 安全审计钩子 | Low | 单点 | Phase 1 |

---

## Phase 1: 快速清除（预计 3.5h）

### B3 — 性能基准编译修复

**根因:** 前次编译被中断导致 `target/release/deps/` 目录结构损坏。

**修复:**
1. `cargo clean --release`
2. 逐一验证三个基准的编译：
   ```bash
   SQLX_OFFLINE=true cargo bench --bench performance_api_benchmarks --no-run
   SQLX_OFFLINE=true cargo bench --bench performance_federation_benchmarks --no-run
   SQLX_OFFLINE=true cargo bench --bench performance_sliding_sync_benchmarks --no-run
   ```
3. 在 `.github/workflows/benchmark.yml` 的 build step 前添加 `cargo clean --release`

**验证:** 三个基准均通过 `--no-run` 编译。

**风险:** 零。仅清理构建缓存，无代码变更。

### B2 — 集成测试一键环境

**根因:** 1196 个集成测试依赖外部 PostgreSQL，新开发者需手动执行 5+ 条命令。

**修复:**
1. 创建 `scripts/dev-test-setup.sh`:
   ```bash
   #!/bin/bash
   set -euo pipefail
   CONTAINER_NAME="synapse-test-db"

   case "${1:-up}" in
     up)
       docker run -d --name "$CONTAINER_NAME" \
         -e POSTGRES_USER=synapse -e POSTGRES_PASSWORD=synapse \
         -e POSTGRES_DB=synapse_test -p 5432:5432 postgres:16
       until docker exec "$CONTAINER_NAME" pg_isready -U synapse >/dev/null 2>&1; do
         sleep 1
       done
       bash docker/db_migrate.sh migrate
       echo "Ready. Run:"
       echo "  export TEST_DB_TEMPLATE_SCHEMA=public"
       echo "  SQLX_OFFLINE=true cargo test --features test-utils --test integration -- --test-threads=2"
       ;;
     down)
       docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
       ;;
   esac
   ```
2. 更新 `TESTING.md` 第 3.2 节，顶部添加快速启动说明（3 行）。

**验证:** 在有 Docker 的机器上执行 `bash scripts/dev-test-setup.sh up` 后能运行集成测试。

**风险:** 低。Docker 依赖已存在于项目中，脚本仅封装现有命令。

### I1 — Clippy 零警告

**根因:** `capability_governance.rs` 中 `governance()` 方法是预留 API，`presence_service.rs` 中返回类型是裸元组。

**修复:**
1. `capability_governance.rs:131` — 已有 `#[allow(dead_code)]`，保持不变。
2. `presence_service.rs` — 定义类型别名：
   ```rust
   pub type PresenceRecord = (String, Option<String>, Option<i64>);
   pub type PresenceBatchRecord = (String, String, Option<String>, Option<i64>);
   ```
   替换 `get_presence_with_meta` 和 `get_presence_batch_with_meta` 的返回类型，移除 `#[allow(clippy::type_complexity)]`。

**验证:** `SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings` 零输出。

**风险:** 零。类型别名是编译期替换，不影响 ABI。

### I6 — Pre-commit 安全钩子

**根因:** CI 有 `security-audit` job，但开发者本地无对应检查。

**修复:**
1. 创建 `.githooks/pre-commit`:
   ```bash
   #!/bin/bash
   cargo audit --quiet 2>/dev/null || true
   ```
2. 创建 `.githooks/pre-push`:
   ```bash
   #!/bin/bash
   cargo deny check advisories
   ```
3. 在 CLAUDE.md 工具链章节添加启用说明。

**验证:** `git config core.hooksPath .githooks` 后提交会触发审计。

**风险:** 低。pre-commit 不阻断，pre-push 仅在推送时触发。

---

## Phase 2: 错误处理与安全加固（预计 2.5h）

### I2 — Service 层错误类型细化

**根因:** key_rotation.rs 和 tags.rs 中所有错误都用 `ApiError::internal()` 映射为 500。

**修复:**
1. 在 `synapse-services/src/room/tags.rs` 中定义 `TagsError` 枚举，实现 `Into<ApiError>`：
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum TagsError {
       #[error("Tag not found")]
       NotFound,
       #[error("Tag already exists")]
       Duplicate,
   }
   ```
   映射规则：NotFound→404，Duplicate→409。
2. 在 `synapse-services/src/key_rotation_service.rs`（如存在）或对应的 handler 中做同样处理。
3. Service 层返回 `Result<T, TagsError>`，handler 层通过 `?` 自动转换为 `ApiError`。

**范围:** 仅 key_rotation.rs 和 tags.rs，不在全项目强制推广。此模式可作为后续 service 开发的参考。

**验证:** 单元测试覆盖 NotFound→404 和 Duplicate→409 的映射。

**风险:** 低。错误类型新增，404→500 的映射变更需要确认无客户端依赖特定 500 行为。

### I3 — media.rs 静默错误修复

**根因:** `resp.text().await.unwrap_or_default()` 在读取失败时返回空字符串，丢失错误上下文。

**修复（2 行）:**
```rust
// line 409 — 改前
let body = resp.text().await.unwrap_or_default();
// 改后
let body = resp.text().await.unwrap_or_else(|e| {
    format!("Failed to read remote media response: {e}")
});

// line 443 — 同
```

**验证:** 单元测试模拟 HTTP 响应体读取失败场景，验证返回的错误消息包含 "Failed to read"。

**风险:** 零。仅改变错误时的字符串内容。

### I4 — LocalhostGuard 提取器

**根因:** admin/register.rs 中 IP 白名单检查分散在多个函数中。

**修复:**
1. 新建 `src/web/routes/extractors/localhost_guard.rs`，实现 `FromRequestParts`:
   - 复用现有 `is_local_registration_origin`、`is_local_registration_host`、`request_targets_localhost`、`is_local_proxy_ip` 逻辑
   - 非本地请求返回 403 `"Admin registration is only available from localhost"`
2. `admin/register.rs` handler 签名：将内联 IP 检查替换为 `_guard: LocalhostGuard` 提取器参数。
3. 保留原有函数作为 LocalhostGuard 的内部实现，已有单元测试继续有效。

**范围:** 仅替换 admin/register.rs 中的检查。其他 admin 端点已有 admin_auth_middleware 保护，不动。

**验证:** 已有单元测试（`is_local_registration_origin` 等）继续通过。

**风险:** 低。IP 检查逻辑等价，行为不变。

---

## Phase 3: 覆盖率提升与路由精简（预计 5 天）

### B1 — 覆盖率 20%→40%

**策略:** 按风险优先级逐模块补充单元测试。风险 = 变更频率 × 影响面。

| 优先级 | 模块 | 当前覆盖 | 目标 | 测试内容 |
|--------|------|---------|------|---------|
| P1 | `extractors/auth.rs` | ~0% | 80% | 认证提取器：MissingToken/UnknownToken/AdminUser |
| P1 | `services/room/service.rs` | 低 | 60% | 房间核心逻辑：创建/加入/离开/邀请 |
| P2 | `services/room/tags.rs` | 0% | 80% | 标签 CRUD + 错误映射 |
| P2 | `services/presence_service.rs` | 低 | 60% | Presence 逻辑 + 订阅管理 |
| P3 | `routes/media.rs` error paths | 低 | 50% | 媒体错误传播路径 |
| P3 | `routes/admin/register.rs` | ~30% | 60% | IP 检查 + nonce 验证 |

**方法:**
1. 用 `cargo tarpaulin --out Html` 生成基线
2. 按优先级 `#[cfg(test)] mod tests` 增量添加
3. Mock storage 层，不依赖数据库
4. 每模块完成后 `cargo test --test unit` 验证
5. 每 +5% 覆盖率 checkpoint commit

**测试编写原则:**
- 优先错误路径 → 业务分支 → API handler → 纯函数
- 命名：`test_auth_extractor_rejects_missing_token`
- 禁止弱断言 `expect(x).is_ok()`

**风险:** 中。新增测试可能暴露隐藏 bug，策略：发现即记录，不阻塞覆盖率提升。

### I5 — r0 路由废弃

**策略:** 三步走，不删代码。

**Step 1:** 为 r0 GET 路由添加 308 重定向到 v3：
```rust
// 仅对无 body 的 GET 请求
.route("/_matrix/client/r0/versions", get(|uri: Uri| async move {
    Redirect::permanent(&uri.to_string().replace("/r0/", "/v3/"))
}))
```
范围：~15 个 client API GET 路由。POST/PUT 不重定向（body 不跟随 308）。

**Step 2:** RouteLedger manifest 中标记 r0 路由为 deprecated，启动时打印 `WARN {} r0 routes are deprecated and will be removed`。

**Step 3:** POST/PUT r0 路由保留不新增。在 CLAUDE.md 中记录迁移计划。

**风险:** 中。旧客户端收到 308 需正确处理重定向。Matrix 规范已废弃 r0 多年。

---

## 验证策略

每阶段结束执行：

```bash
# 主门禁
cargo fmt --all -- --check
SQLX_OFFLINE=true cargo clippy --all-features --locked -- -D warnings
cargo test --doc --locked
SQLX_OFFLINE=true cargo test --test unit --features test-utils --locked

# Phase 2 额外
cargo test --test unit --features test-utils --locked  # 含新增错误映射测试

# Phase 3 额外
cargo tarpaulin --out Html --output-dir coverage  # 验证覆盖率提升
```

## 交付物

| 阶段 | 产出 | 类型 |
|------|------|------|
| Phase 1 | 4 个 commit (B3/B2/I1/I6) | 代码 |
| Phase 2 | 3 个 commit (I2/I3/I4) | 代码 |
| Phase 3 | 6+ 个 commit (每个模块测试 + I5) | 代码 |
| 全局 | `docs/superpowers/specs/2026-06-28-qa-optimization-design.md` | 设计文档 |
| 全局 | Final QA re-baseline report | HTML |

## 回滚策略

每个问题独立 commit，可单独 revert。如某阶段引入回归：
1. `git revert <commit>` 撤销单个修复
2. 重新运行验证命令
3. 在下一次迭代中修复并重新提交
