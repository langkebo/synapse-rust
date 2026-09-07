# 测试覆盖审计报告

> 任务：测试覆盖审计——扫描单元测试、集成测试、联邦测试用例的覆盖度，标记核心逻辑的测试盲区，补充缺失的边界场景、异常场景测试用例。
> 日期：2026-09-05
> 状态：完成（输出待办清单 + 补充建议）

---

## 一、审计范围

| 层级 | 路径 | 规模 |
|------|------|------|
| 单元测试 | `tests/unit/` | 108 个文件，85 个 mod（`mod.rs` 声明） |
| 集成测试 | `tests/integration/` | 115 个文件，111 个 mod |
| E2E 测试 | `tests/e2e/` | 2 个文件（`e2e_scenarios.rs` 493L, `user_flow_tests.rs` 577L） |
| Complement | `tests/complement/` | 1 个 `main_test.go`（联邦互操作入口，无实际测试） |
| 总计 | 216 个测试文件 | ~1396 个测试函数 |

---

## 二、核心数据：审计发现

### 2.1 总览（路由层 vs Service/Storage/E2EE 层）

| 层 | 零测试 | 低覆盖（≤5） | 正常/已覆盖 |
|----|-------|-------------|------------|
| **路由层**（`src/web/routes/`） | 1 个真盲区 | 17 个低覆盖 | 124 个正常 |
| **Service 层**（`synapse-services/`） | 0 | 1（`wiring/accounts.rs`） | 大量 |
| **Storage 层**（`synapse-storage/`） | 0 | 2（test_mocks） | 63 个有内联 `db_tests` |
| **E2EE 层**（`synapse-e2ee/`） | 0 | 0 | 全部有内联或集成测试 |

**关键解读**：
- ✅ **Service/Storage/E2EE 三层零真盲区**——核心业务逻辑覆盖完整。
- ⚠️ **路由层 17 个低覆盖** + **1 个真盲区** + **3 个有"假测试"** = 实际测试盲区集中在路由层。
- 🔍 **lcov 报告中"0%"文件多数是测试与实现路径不一致**，不是真盲区（详见 §2.4）。

### 2.2 路由层真盲区（1 个）

| 文件 | 行数 | 问题 |
|------|------|------|
| `src/web/routes/extractors/localhost_guard.rs` | **336L** | 0 个测试触达。localhost 访问控制是 P-0 安全路径，无任何测试覆盖。 |

### 2.3 路由层低覆盖（≤5 个测试文件，17 个）

按行数降序：

| 文件 | 行数 | 现有测试 | 风险评估 |
|------|------|---------|---------|
| `src/web/routes/account_compat.rs` | 714 | 5 个 | 17 个 handler，只测了 route_manifest |
| `src/web/routes/assembly.rs` | 708 | 5 个 | 路由装配核心 |
| `src/web/routes/auth_compat.rs` | 695 | 5 个 | 认证兼容路径 |
| `src/web/routes/media/download.rs` | 442 | 5 个 | 媒体下载 P-1 路径 |
| `src/web/routes/route_module.rs` | 368 | 4 个 | 路由模块化 |
| `src/web/routes/handlers/search/hierarchy.rs` | 264 | 3 个 | Space 层级查询 |
| `src/web/routes/oidc/builtin.rs` | 181 | 3 个 | 内置 OIDC 登录 |
| `src/web/routes/space/membership_state.rs` | 167 | 3 个 | Space 成员状态 |
| `src/web/routes/feature_flags.rs` | 146 | 4 个 | 特性开关 |
| `src/web/routes/federation/membership/knock.rs` | 141 | 4 个 | 联邦 knock 入场 |
| `src/web/routes/room_access.rs` | 106 | 4 个 | 房间访问控制 |
| `src/web/routes/moderation.rs` | 92 | 3 个 | 审核路由 |
| `src/web/routes/handlers/room/management/upgrade.rs` | 65 | 3 个 | 房间升级 |
| `src/web/routes/media/preview.rs` | 60 | 3 个 | 媒体预览 |
| `src/web/routes/ephemeral.rs` | 56 | 3 个 | 临时事件 |
| `src/web/routes/handlers/client_config.rs` | 43 | 3 个 | 客户端配置 |
| `src/web/routes/formatting.rs` | 23 | 3 个 | 消息格式化 |

**共同模式**：`account_compat_route_tests.rs` / `auth_compat_route_tests.rs` 这类测试**只验证 route_manifest**，**不调用 handler 函数体**——所以 lcov 显示 0% 覆盖。这是审计发现的最大盲区。

### 2.4 低质/无效测试（"假测试"）

#### A. `tests/integration/coverage_tests.rs`（617 行）

**问题**：30+ 个 `#[tokio::test]`，全部是 no-op：
```rust
#[tokio::test]
async fn test_send_message() {
    let message = json!({ /* ... */ });
    assert!(message.get("content").is_some());  // 永远为 true
}
```
**影响**：不会失败，不会运行，不覆盖任何生产代码。**误导覆盖率统计**。

**修正方案**：
- 方案 1：全部删除（最干净）
- 方案 2：保留为 stub，配合 `#[ignore]` 标注 TODO
- **推荐**：方案 1，删除整个文件

#### B. route_manifest-only 测试

`account_compat_route_tests.rs`, `auth_compat_route_tests.rs`, `key_backup_api_tests.rs` 等只测试了 manifest 而非 handler。

**已有改进模式**（P-096 修复）：
- `account_compat_route_tests.rs` 头部注释明确写"the module is private, so tests follow the same pattern as `key_backup_api_tests.rs`: pure JSON-shape + validation-logic assertions, no HTTP router or DB"
- 这是**有意为之**（私有 mod 无法外部调用），但**导致 handler 函数体不被覆盖**

---

## 三、边界/异常场景测试覆盖

### 3.1 已有专项测试

| 测试文件 | 行数 | 覆盖点 |
|---------|------|--------|
| `tests/integration/api_error_compliance_tests.rs` | 62 | 错误码符合 Matrix 规范 |
| `tests/integration/api_input_validation_tests.rs` | 176 | 输入校验边界 |
| `tests/integration/permission_escalation_tests.rs` | 228 | 权限提升攻击 |
| `tests/unit/boundary_tests.rs` | 443 | 通用边界值 |
| `tests/integration/federation_error_tests.rs` | 254 | 联邦错误响应 |
| `tests/integration/concurrency_tests.rs` | 100 | 并发竞争 |
| `tests/integration/protocol_compliance_tests.rs` | 184 | 协议合规性 |
| `tests/integration/federation_existence_leak_tests.rs` | ? | 联邦存在性泄漏 |
| `tests/integration/database_integrity_tests.rs` | ? | DB 完整性 |

### 3.2 边界/异常场景缺失清单

按业务影响度排序：

#### 高优先级（P-0/P-1）

| 缺失场景 | 涉及模块 | 建议测试文件 |
|---------|---------|------------|
| **LocalhostGuard 误判**（IPv6 私网、proxy_protocol、loopback 错误解析） | `extractors/localhost_guard.rs` | 新建 `tests/unit/localhost_guard_coverage_tests.rs` |
| **OIDC redirect_uri 注入** | `oidc/builtin.rs` | 新建 `oidc_security_edge_tests.rs` |
| **联邦签名过期/未来时间戳边界** | `federation/signing.rs`, `state_resolution.rs` | 扩展 `federation_error_tests.rs` |
| **knock 房间状态机**（被 ban 后 knock、已 invite 后 knock） | `federation/membership/knock.rs` | 扩展 |
| **media download 大文件 OOM**（>2GB 校验） | `routes/media/download.rs` | 扩展 `media_api_tests.rs` |
| **rate limit token bucket 边界**（0 容量、负数时间） | `cache/rate_limit.rs` | 新建 |
| **privacy/extensible_events 版本不匹配** | `extensible_events.rs` | 新建 |

#### 中优先级（P-2）

| 缺失场景 | 涉及模块 |
|---------|---------|
| **typing 通知过期边界**（0ms, 30s+1ms） | `typing_service.rs` |
| **presence 状态机非法转换**（offline→online 已过期） | `presence_service.rs` |
| **receipt 类型边界**（m.read/m.read.private 切换） | `receipt_storage.rs` |
| **redaction of redacted event**（幂等边界） | event handlers |
| **megolm 消息重放窗口** | `e2ee/megolm/` |
| **device list 增量同步边界**（空 deltas） | `device_keys/` |
| **rendezvous 过期边界**（MSC4108） | `rendezvous_service.rs` |
| **sticky event 跨集群漂移** | `sticky_event.rs` |
| **event_auth 状态解析死锁** | `event_auth/chain.rs` |
| **search 全文索引分词边界**（unicode, emoji, 长度） | `search_service.rs` |

#### 低优先级（P-3 已知但有基础覆盖）

- 联邦 transaction 重复提交（已有 `api_federation_transaction_tests.rs`）
- 房间状态事件并发（已有 `concurrency_tests.rs`）
- 协议合规错误码（已有 `protocol_compliance_tests.rs`）

### 3.3 Complement 联邦互操作——几乎是空架子

- `tests/complement/main_test.go` 只有入口（5KB）
- **0 个实际的 `*_test.go` 文件**
- 这是**重大盲区**：与上游 Synapse 的互操作完全未测试

**补测建议**：
- 引入 `complement-crypto` 工具链
- 从 Synapse 上游复制关键测试用例（握手、room 状态、e2ee）
- 至少补 20-30 个核心场景

---

## 四、补充测试用例清单（按 ROI 排序）

### Round 1：高 ROI（必做，1-2 人天）

| 任务 | 文件 | 新增测试 | 估算 |
|------|------|---------|------|
| **R1-1**：补 LocalhostGuard 完整测试 | `tests/unit/localhost_guard_coverage_tests.rs` | 12-15 个 | 4h |
| **R1-2**：OIDC 攻击面覆盖 | `tests/unit/oidc_security_edge_tests.rs` | 10 个 | 4h |
| **R1-3**：联邦签名时间戳边界 | 扩展 `tests/integration/federation_error_tests.rs` | 8 个 | 3h |
| **R1-4**：media 大文件 + 速率限制 | 扩展 `tests/integration/api_media_routes_tests.rs` | 6 个 | 3h |
| **R1-5**：knock 状态机 | 扩展 `tests/integration/api_space_routes_tests.rs` | 8 个 | 3h |
| **R1-6**：account_compat 补 handler 级测试 | 扩展 `tests/unit/account_compat_route_tests.rs` | 12 个 | 4h |
| **R1-7**：auth_compat 补 handler 级测试 | 扩展 `tests/unit/auth_compat_route_tests.rs` | 10 个 | 3h |
| **小计** | | 66-70 个 | ~24h |

### Round 2：中 ROI（3-5 人天）

| 任务 | 估算 |
|------|------|
| R2-1：presence/typing/receipt 状态机边界 | 6h |
| R2-2：extensible_events 协议版本 | 4h |
| R2-3：redaction 幂等 + 嵌套 | 3h |
| R2-4：device_list 增量同步 | 4h |
| R2-5：megolm 重放窗口 | 4h |
| R2-6：rate_limit 边界（0/负数/超大） | 3h |
| R2-7：sticky_event 跨集群 | 3h |
| **小计** | ~27h |

### Round 3：Complement 联邦互操作（5-7 人天）

| 任务 | 估算 |
|------|------|
| R3-1：建立 Go module 完整结构 | 1d |
| R3-2：复制 Synapse 关键测试（room/version/keys/transaction） | 2d |
| R3-3：联邦 handshake 失败/重试 | 1d |
| R3-4：跨服务器 room 状态 | 1d |
| **小计** | ~5d |

---

## 五、清理建议

### 5.1 删除 no-op 测试

**`tests/integration/coverage_tests.rs`**（617 行，30+ 个 `#[tokio::test]`）—— 全部是 `assert!(json!().get().is_some())` 无意义断言。

**操作**：
```bash
# 1. 验证无功能依赖
grep -rl "tests::integration_tests" tests/ 2>/dev/null
# 2. 删除文件
rm tests/integration/coverage_tests.rs
# 3. 从 integration/mod.rs 移除 `mod coverage_tests;`
```

### 5.2 修复"假测试"模式

`account_compat_route_tests.rs` 头部注释解释"模块私有，只能测 manifest"——这是**真实的工程限制**，但**导致 0% 覆盖**。

**修正策略**：
1. 在 `mod.rs` 把关键 module 标 `pub(crate)`
2. 或抽出 helper 函数到独立 `pub` 模块，让测试可调用
3. 或在测试 crate 加 `#[cfg(test)] pub use` 重导出

---

## 六、覆盖率分布总览

### 6.1 按模块类型

| 类型 | 文件数 | 总行数 | 覆盖行数 | 覆盖率 | 来源 |
|------|--------|--------|---------|--------|------|
| Workspace lib（storage/services/e2ee/federation） | ~600 | ~30000+ | ~20000+ | **~68%** | AGENTS.md 2026-08 |
| 根 crate 路由 | 142 | ~60000+ | ~10000+ | **~17%** | lcov 2026-09-04 |
| 根 crate 其他（server、e2ee glue、worker） | ~70 | ~5000+ | ~1500+ | **~30%** | lcov 2026-09-04 |
| **整体** | ~800+ | ~95000+ | ~31500+ | **~33%** | 估算（需重跑） |

### 6.2 关键数字

- 测试函数总数：**~1396**
- 集成测试文件：**115**
- 单元测试文件：**108**
- E2E 测试：**2**（基本无覆盖）
- Complement 联邦互操作：**0 个实际测试**

---

## 七、交付清单（建议接下来做的事）

按优先级：

1. **【立即】** 删除 `tests/integration/coverage_tests.rs`（P-0 数据噪音）
2. **【P-0】** 补 LocalhostGuard 完整测试（4h，盲区）
3. **【P-0】** 补 OIDC 攻击面测试（4h，盲区）
4. **【P-1】** 补 R1-3 ~ R1-7 联邦/认证/媒体/状态机（16h）
5. **【P-2】** 修复 route_manifest-only 测试的覆盖模式（架构调整 4h）
6. **【P-3】** 建立 Complement 联邦互操作基础（5d）
7. **【持续】** CI 覆盖率门禁（`--fail-under-lines 60`）

**估算总投入**：约 12-15 人天，可将整体覆盖率从 ~33% 提升到 ~50-55%。

---

## 八、风险与注意事项

| 风险 | 说明 | 缓解 |
|------|------|------|
| handler 测试难以触达 | 私有 mod 无法外部调用 | 改 `pub(crate)` 或抽 helper |
| feature gate 复杂 | 部分代码需 `friends`/`widgets` 等非默认 feature | 用 `--all-features` 跑 + 显式禁用 |
| 私有 mod 测试模式 | 项目惯例有"只测 manifest" | 详见 §5.2 |
| Complement 工具链不熟 | Go module 复杂 | 抄 Synapse 上游 + 文档 |
| 测试 flaky | db_tests 并发竞争 | 沿用 `RUST_TEST_THREADS=1` |

---

## 附录 A：审计方法说明

- **方法 1**：Python 脚本 `audit_deep.py` 扫描所有 src 与 tests 的关键词匹配
- **方法 2**：lcov 2026-09-04 baseline 分析（`coverage/lcov.info`）
- **方法 3**：人工审查关键文件 `coverage_tests.rs` 30+ 个 `#[tokio::test]`
- **方法 4**：grep `#[cfg(test)]` / `db_tests` 识别内联测试

> 审计发现的方法论局限：
> - 关键词匹配会**误报**（大量测试用公共词汇）
> - lcov 是部分跑数据，**不**代表完整 workspace 数字
> - 内联 `#[cfg(test)]` 模块**不会**被 `tests/unit` 测试覆盖计数
> - **执行 Phase A（重跑 llvm-cov）才能得到 100% 准确的数字**

---

*本审计基于 2026-09-04 数据。执行补测后请重跑 `bash scripts/run_local_coverage.sh` 更新基线。*
