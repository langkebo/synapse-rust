---
name: tdd-rust
description: synapse-rust 项目级 TDD 工作流。在写或改 storage/services/federation/routes 任意生产代码前强制触发 Red-Green-Refactor 循环，绑定 cargo test / cargo nextest 命令，使用 insta 快照与预置 Mock 适配器。
---

# synapse-rust 项目级 TDD 工作流

> 适用范围：`synapse-storage` / `synapse-services` / `synapse-federation` / `synapse-e2ee` / `src/web/routes` 任意生产代码修改。
> 工具链：cargo 1.93.0 · sqlx 0.8 · mockall 0.13 · wiremock 0.6 · insta 1.x · nextest (可选)。
> 既有可复用资产：`FakeUserStore`（内存适配器模式）、`auth::trait::Auth`（服务 trait 抽象）、`common::traits::EventBroadcaster`（关联类型 trait）、`synapse-storage::test_utils`（隔离测试 schema 池）。

## 1. 触发条件（强制）

下列任意一项出现时，**必须**进入 TDD 流程，禁止先改生产代码：

1. 用户请求新增 storage/service/route 方法或新行为。
2. 修复 bug 时已能写出最小复现描述。
3. 用户显式提到 "TDD" / "red-green-refactor" / "测试先行" / "test-first"。
4. 修改 `Cargo.toml` 引入新公共 API 签名。

**例外**（允许跳过 Red）：
- 纯文档/注释/格式化 (`cargo fmt --all`)。
- 依赖版本号升级且无 API 变化。
- 配置字段重命名且无行为变化。

## 2. 强制校验门 (Red-Green-Refactor)

每个循环必须按以下顺序产出可验证工件，禁止合并多个测试一次性写完（"横向切片"反模式）：

```
RED     → 写 1 个测试 → 失败原因必须是 "未实现" 或 "panic"，而非编译错误
GREEN   → 写刚好让该测试通过的最小生产代码，禁止预写下个测试需要的逻辑
REFACTOR→ 全绿后才能动；每步重构后立即重跑同一测试
```

### 2.1 每循环自检清单

- [ ] 测试只走 public interface（route → service → storage），未 `mod tests` 内省私有字段。
- [ ] 测试名描述行为：`can_register_user_with_valid_password`，而非 `test_register_1`。
- [ ] Mock 用 `mockall::automock` 或预置内存适配器（见 §4），未直接 `unwrap()` 真实 PgPool。
- [ ] 失败信息包含业务上下文，而非裸 `assert_eq!`。
- [ ] GREEN 阶段未引入未使用的参数/字段。
- [ ] 重构后 `cargo test <new_test> -- --exact` 仍绿。

## 3. Cargo 命令绑定（自动触发流程）

按以下命令映射各阶段，禁止用 `cargo test` 全量跑作为 RED 验证（太慢）：

| 阶段 | 命令 | 用途 |
|------|------|------|
| RED 验证（单测） | `cargo test --features test-utils --test unit <test_name> -- --exact --nocapture` | 验证新测试失败 |
| RED 验证（lib 内联） | `cargo test --lib <module>::<test_name> -- --exact --nocapture` | 验证 inline `#[cfg(test)]` 失败 |
| GREEN 验证 | 同上 | 验证最小实现通过 |
| 集成回归 | `cargo test --features test-utils --test integration <module> -- --nocapture` | 防止破坏既有契约 |
| 快照验收 | `cargo insta review` （首次写）/ `cargo test --features test-utils --test unit <snap_test>` （验收） | API 输出格式锁定 |
| 全量门禁 | `TEST_THREADS=4 TEST_RETRIES=2 bash scripts/run_ci_tests.sh` | 提交前最终校验 |
| 覆盖率 | `cargo tarpaulin --output-dir coverage/ --html --skip-clean` | 周期性覆盖率回归 |
| 编译检查 | `cargo clippy --all-features --locked -- -D warnings` | 类型/lint 不破 |

**首选 nextest**：若装了 `cargo-nextest`，RED/GREEN 用 `cargo nextest run -p <crate> <test_name>`，比 cargo test 快 2-3 倍。

### 3.1 提交前必须全绿的最小子集

```bash
cargo fmt --all -- --check
cargo clippy --all-features --locked -- -D warnings
cargo test --features test-utils --test unit -- --test-threads=4
```

仅这 3 项全绿方可进入 `scripts/run_ci_tests.sh`（带集成测试的完整 CI 等价）。

## 4. Mock 适配器使用规则

预置 Mock 适配器位于各 crate 的 `test_mocks` 模块（见 [执行清单](../../../.trae/documents/TDD落地执行清单.md)）：

| Crate | Mock 入口 | 适用场景 |
|-------|----------|----------|
| `synapse-storage` | `test_mocks::InMemoryUserStore` / `InMemoryRoomStorage` | service 层单测，无需 PgPool |
| `synapse-federation` | `test_mocks::MockFederationClient` | 联邦路由/service 测试，无 HTTP 出站 |
| `synapse-services` | `test_mocks::MockSyncServiceDeps` | sync / sliding_sync 单测，无真实 storage |

### 4.1 选用决策树

```
被测代码依赖 ─→ 是否 trait 化？
   ├─ 是  ─→ mockall::automock 还是手写内存适配器？
   │        ├─ 方法 ≤5 且无关联类型  → #[automock] 自动生成
   │        └─ 复杂状态/关联类型     → 手写 Fake* (参考 FakeUserStore)
   └─ 否  ─→ 提取 trait 或注入 Mock*Dep（参考 SyncServiceDeps）
              禁止在测试中 #[patch] 私有字段
```

### 4.2 禁止反模式

- ❌ 在测试中直接 `query!("SELECT ...")` 验证 storage 行为（应走 public method）。
- ❌ 用 `PgPool::connect` 跑 unit test（unit 必须无 DB；DB 测试归 integration）。
- ❌ Mock 整个 ServiceContainer（应只 Mock 直接依赖的 storage trait）。
- ❌ 用 `#[cfg(test)]` 暴露生产代码私有字段供测试断言。

## 5. insta 快照测试规则

适用于 **API 路由返回值** 与 **可序列化的服务输出**。参考实现：
[tests/integration/api_route_snapshots_tests.rs](../../../tests/integration/api_route_snapshots_tests.rs)。

### 5.1 存放约定（P2-7）

| 测试类型 | 快照目录 | 命名规则 |
|---------|---------|---------|
| 集成测试（路由） | `tests/integration/snapshots/` | `<test_name>.snap` |
| 单元测试（服务/storage） | `tests/unit/snapshots/` 或 crate 内 `src/snapshots/` | `<module>__<test_name>.snap` |
| crate 内联测试 | `<crate>/src/snapshots/` | 与 `#[cfg(test)]` 模块同 crate |

- 快照文件必须提交到仓库，禁止 `.gitignore`。
- 命名使用 `assert_json_snapshot!("snapshot_name", value)` 显式指定，避免自动名称不稳定。

### 5.2 动态字段 redaction 规则（P2-6）

以下字段在快照中必须 redact，禁止裸写入：

| 字段 | redaction 占位 | 原因 |
|------|----------------|------|
| `access_token` | `[redacted_access_token]` | JWT，每次签发不同 |
| `refresh_token` | `[redacted_refresh_token]` | UUID，每次签发不同 |
| `expires_in` | `[redacted_expires_in]` | 可能因配置变化 |
| `device_id` | `[redacted_device_id]` | 客户端生成或服务器随机 |
| `user_id` | `[redacted_user_id]` | 含随机后缀（admin_<rand>） |
| `session`（UIA） | `[redacted_session_uuid]` | UUID v4 |
| `origin_server_ts` | `[redacted_ts]` | 毫秒时间戳 |
| `joined_ts` | `[redacted_ts]` | 毫秒时间戳 |
| `identity_providers`（SSO） | `[redacted_sso_providers]` | 依赖 feature flags |

### 5.3 三种 redaction 实现方式（按优先级）

#### 方式 A：inline redaction（推荐，简单字段）

```rust
insta::assert_json_snapshot!("register_uia_401_challenge", body, {
    ".session" => "[redacted_session_uuid]",
    ".access_token" => "[redacted_access_token]",
});
```

#### 方式 B：JSON 预处理（推荐，复杂/嵌套字段）

```rust
let (status, mut body) = send_request(app, "GET", "/.../login", None).await;
if let Some(flows) = body.get_mut("flows").and_then(|v| v.as_array_mut()) {
    for flow in flows.iter_mut() {
        if let Some(obj) = flow.as_object_mut() {
            if obj.contains_key("identity_providers") {
                obj.insert("identity_providers".to_string(),
                    serde_json::Value::String("[redacted_sso_providers]".into()));
            }
        }
    }
}
insta::assert_json_snapshot!("login_flows_v3", body);
```

#### 方式 C：regex filters（旧式，用于全局过滤）

```rust
insta::with_settings!({
    filters => vec![
        (r#""access_token":"[^"]+""#, r#""access_token":"[redacted_access_token]""#),
        (r#""expires_in":\d+"#, r#""expires_in":[redacted_expires_in]"#),
    ]
}, {
    assert_json_snapshot!(resp);
});
```

> **禁止**使用 `insta::dynamic_redaction`（闭包签名要求 `Content` 类型，与 `serde_json::Value` 不兼容，且全局污染）。

### 5.4 规则

- 路由 handler 必须有 ≥1 个 snapshot 测试，覆盖 `Matrix spec` 定义的 `errcode` + 关键字段。
- 快照更新必须 `cargo insta review` 人工确认，禁止 `INSTA_UPDATE=always` 直接入仓。
- CI 用 `cargo insta test --no-review`（非交互），本地用 `cargo insta test --review`。
- 含动态字段必须按 §5.2 redaction 表过滤。
- snapshot 命名必须显式（`assert_json_snapshot!("name", ...)`），禁止隐式自动命名。

## 6. 工作流模板（每循环复制使用）

```
1. [RED]   写测试 → cargo nextest run -p <crate> <name>  → 期望 FAIL
2. [GREEN] 写最小实现                              → 期望 PASS
3. [REFACTOR] cargo clippy + fmt --check         → 期望 PASS
4. [SNAPSHOT] 若改路由返回 → cargo insta review    → 人工 review
5. [REGRESS] cargo test --test integration <module> → 期望 PASS
6. 提交：git commit -m "test(<module>): <behavior>"  // 测试与实现同提交
```

## 7. 失败处理

- **RED 失败为编译错误** → 说明测试在测实现而非行为；重写测试只走 public API。
- **GREEN 一直不能通过** → 重读 §2 的反模式；不要在测试里加 `if cfg!(test)` 分支。
- **重构后测试变红** → 该测试耦合了实现，回到 §2.1 自检清单重写。

## 8. 与既有覆盖率目标的衔接

参考 [测试覆盖率提升至 80% 优化方案 v2](../../../.trae/documents/测试覆盖率提升至80%优化方案-v2.md)。本 SKILL 不重复覆盖率统计方法，仅约束新增代码必须 TDD，以保证覆盖率不再回退。新增代码的覆盖率门禁为 **≥80%**（按文件计）。

## 9. 参考文档

- 通用 TDD 哲学：[.claude/skills/tdd/SKILL.md](../tdd/SKILL.md)
- Mocking 指南：[.claude/skills/tdd/mocking.md](../tdd/mocking.md)
- 重构模式：[.claude/skills/tdd/refactoring.md](../tdd/refactoring.md)
- 项目规则：[.trae/rules/project_rules.md](../../../.trae/rules/project_rules.md) 第九节测试规范
- 执行清单：[.trae/documents/TDD落地执行清单.md](../../../.trae/documents/TDD落地执行清单.md)
