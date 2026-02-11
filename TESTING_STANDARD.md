# Synapse 项目测试标准规范

本文档定义了 Synapse 项目的测试分层策略、命名规范和执行标准，旨在构建全面、健壮的质量保障体系。

## 1. 测试分层策略

我们遵循测试金字塔原则，但根据项目特性（Rust + 数据库重依赖）进行了适配。

### 1.1 单元测试 (Unit Tests)
*   **位置**: `src/**/mod.rs` 或 `src/**/tests.rs` 中的 `#[cfg(test)]` 模块。
*   **目标**: 验证纯函数逻辑、工具类、解析器和验证器。
*   **原则**:
    *   **零外部依赖**: 不连接真实数据库、Redis 或外部 HTTP 服务。
    *   **极速执行**: 单个测试应在毫秒级完成。
    *   **Mocking**: 使用 `mockall` 或手动 Mock 隔离 IO 操作。
*   **示例**: `src/common/validation.rs` 中的正则验证测试。

### 1.2 服务集成测试 (Service Integration Tests)
*   **位置**: `tests/unit/` (历史遗留命名，实际为服务集成测试)。
*   **目标**: 验证 Service 层业务逻辑与数据库的交互。
*   **原则**:
    *   **真实数据库**: 连接测试用的 PostgreSQL 实例。
    *   **事务隔离**: 每个测试应在独立事务中运行，或在测试后清理数据。
*   **示例**: `tests/unit/room_service_tests.rs`。

### 1.3 API 集成测试 (API Integration Tests)
*   **位置**: `tests/integration/`。
*   **目标**: 验证 HTTP API 端点的行为（请求处理、路由、中间件、状态码）。
*   **原则**:
    *   **黑盒/灰盒**: 通过 HTTP Client 发送请求，检查 Response 和数据库状态。
    *   **覆盖率**: 必须覆盖正常路径 (Happy Path) 和常见错误路径 (Error Path)。
*   **示例**: `tests/integration/api_room_tests.rs`。

### 1.4 端到端测试 (E2E Tests)
*   **位置**: `tests/e2e/`。
*   **目标**: 模拟真实用户场景，验证跨模块协作。
*   **原则**:
    *   **全流程**: 注册 -> 登录 -> 业务操作 -> 登出。
    *   **关键路径**: 覆盖核心业务价值流。

### 1.5 性能与属性测试 (Advanced Tests)
*   **属性测试**: 使用 `quickcheck` 自动生成边缘数据测试纯逻辑（如 `src/common/validation.rs`）。
*   **性能测试**: `tests/performance/` 中的基准测试，监控关键 API 的延迟和吞吐量。

## 2. CI/CD 集成

所有代码变更必须通过以下 CI 检查：

1.  **编译检查**: `cargo check`
2.  **代码格式**: `cargo fmt -- --check`
3.  **静态分析**: `cargo clippy`
4.  **测试执行**:
    *   `cargo test` (运行 Unit + Service + API Tests)
    *   `newman run tests/api_tests.json` (API 冒烟测试)
5.  **覆盖率门禁**: 目标 80%（使用 `cargo tarpaulin`）。

## 3. 编写规范

*   **命名**: 测试函数名应清晰描述测试场景，如 `test_create_room_with_invalid_name_should_fail`。
*   **断言**: 使用 `assert_eq!`, `assert!(result.is_err())` 等标准宏，配合清晰的错误消息。
*   **清理**: 确保测试产生的副作用（数据库记录）被清理，推荐使用事务回滚。

## 4. 改进路线图

*   [ ] 引入 `cargo-fuzz` 对协议解析进行模糊测试。
*   [ ] 增加 Mock 框架的使用，减少对 Service Integration Tests 的过度依赖，提升测试速度。
*   [ ] 完善 Security Testing Suite，自动化 SQL 注入和 XSS 检测。
