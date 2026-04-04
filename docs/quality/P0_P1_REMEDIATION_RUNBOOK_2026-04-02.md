# P0 / P1 可执行修复清单

## 目标

- 把当前审查出的 P0 / P1 问题展开为可执行动作
- 每项包含执行命令、预期结果、验收方式
- 先落地当前环境可安全执行的项，再记录受实现范围限制的后续项

## 执行原则

- 先修复会直接阻塞测试与 CI 的问题
- 先收敛测试链路，再收敛文档与规则
- 对需要较大认证流程改造的安全项，先保留为后续专项任务

## P0

### P0-1 测试库 schema 对齐

- 状态：已完成
- 目标：让测试连接建立后自动对齐到当前迁移基线，避免 schema smoke test 因旧库结构失败
- 变更位置：
  - `tests/common/mod.rs`
- 执行命令：
  - `cargo test --locked --test unit db_schema_smoke_tests::db_schema_smoke_tests::test_device_trust_schema_smoke_roundtrip -- --exact --nocapture`
  - `cargo test --locked --test unit db_schema_smoke_tests::db_schema_smoke_tests::test_verification_and_moderation_schema_smoke_roundtrip -- --exact --nocapture`
  - `cargo test --locked --test unit -- --test-threads=1`
- 预期结果：
  - schema smoke tests 不再报缺少 `created_ts` 列
  - `unit` 测试重新回到全绿
- 验收方式：
  - 两个 schema smoke tests 通过
  - `cargo test --locked --test unit -- --test-threads=1` 退出码为 0
- 实际结果：
  - 通过在 `tests/common/mod.rs` 中接入测试库初始化逻辑，连接测试库时自动执行运行时迁移对齐
  - `cargo test --locked --test unit -- --test-threads=1` 已通过，结果为 `783 passed; 0 failed`

### P0-2 安全热点后续专项

- 状态：已完成
- 目标：落地 OIDC callback state/PKCE 绑定校验、增强 SAML 响应校验、补强管理员注册来源限制并建立回归测试
- 变更位置：
  - `src/web/routes/oidc.rs`
  - `src/services/saml_service.rs`
  - `src/web/routes/admin/register.rs`
- 执行命令：
  - `cargo test --locked --test unit oidc -- --nocapture`
  - `cargo test --locked --test unit saml -- --nocapture`
  - `cargo test --locked --test integration api_admin -- --nocapture`
- 预期结果：
  - OIDC callback 在消费 state 后执行 PKCE 绑定校验
  - SAML 响应增加状态码、Destination、Recipient、Issuer 一致性校验
  - 管理员注册在本地 IP 限制外增加 Origin/Referer 来源限制
- 验收方式：
  - 新增/更新单元测试可覆盖上述安全校验分支
  - 格式检查与静态检查通过
  - 定向回归命令可复现通过与已知失败项
- 实际结果：
  - 已在 `oidc_callback` 引入 `validate_state_pkce_binding`，并将会话中 `code_challenge`、`code_challenge_method` 纳入绑定校验
  - 已在 `validate_response` 增加 SAML `StatusCode`、`Destination`、`Recipient`、响应 `Issuer` 校验逻辑
  - 已在管理员注册入口增加 `Origin/Referer` 本地来源校验（localhost/loopback）
  - 已补充对应单元测试：
    - OIDC：`test_validate_state_pkce_binding_*`
    - SAML：`test_validate_response_rejects_non_success_status`、`..._mismatched_destination`、`..._mismatched_recipient`
    - Admin Register：`test_ensure_local_admin_registration_request_*`
  - `cargo fmt --all -- --check`、`cargo clippy --all-features --locked -- -D warnings` 均通过
  - `cargo test --locked --test integration api_admin -- --nocapture` 触发既有集成失败（多处 `left: 500, right: 200`），不属于本次新增安全校验分支回归失败

## P1

### P1-1 恢复格式检查通过

- 状态：已完成
- 目标：让 `cargo fmt --all -- --check` 重新通过
- 变更位置：
  - `tests/unit/room_service_tests.rs`
  - `tests/unit/sync_service_tests.rs`
  - 以及 rustfmt 自动整理到的相关文件
- 执行命令：
  - `cargo fmt --all`
  - `cargo fmt --all -- --check`
- 预期结果：
  - 格式检查退出码为 0
- 验收方式：
  - `cargo fmt --all -- --check` 通过
- 实际结果：
  - 已执行 `cargo fmt --all`
  - 已执行 `cargo fmt --all -- --check`，退出码为 0

### P1-2 修正 CI 仓库体检规则误报

- 状态：已完成
- 目标：保留私钥扫描能力，同时允许公开证书文件存在
- 变更位置：
  - `.github/workflows/ci.yml`
- 执行命令：
  - `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); puts "ci.yml: YAML OK"'`
  - `git ls-files "*.key" "*.p12"`
- 预期结果：
  - `repo-sanity` 不再因公开证书 `*.pem` 误报
  - YAML 继续合法
- 验收方式：
  - `ci.yml` 语法通过
  - 规则仍覆盖私钥与高风险制品
- 实际结果：
  - 已将 `git ls-files` 检查收敛到 `*.key` 与 `*.p12`
  - 已执行 YAML 校验，结果为 `workflow yaml: OK`
  - 已执行 `git ls-files '*.key' '*.p12'`，当前无跟踪命中

### P1-3 收敛测试文档到当前工作流

- 状态：已完成
- 目标：把测试文档中的 CI 示例更新到当前 `ci.yml + benchmark.yml`
- 变更位置：
  - `TESTING.md`
- 执行命令：
  - `cargo test --doc --locked`
  - `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/benchmark.yml"); puts "workflow yaml: OK"'`
- 预期结果：
  - 文档不再引用过期的 `.github/workflows/test.yml` 示例
- 验收方式：
  - 文档内容与当前工作流结构一致
  - 文档测试继续通过
- 实际结果：
  - 已将 `TESTING.md` 的 CI 片段更新为当前 `ci.yml + benchmark.yml` 结构
  - 已执行 `cargo test --doc --locked`，退出码为 0
  - 已执行工作流 YAML 校验，退出码为 0

## 实际执行顺序

1. P0-1 测试库 schema 对齐
2. P1-1 恢复格式检查通过
3. P1-2 修正 CI 仓库体检规则误报
4. P1-3 收敛测试文档到当前工作流
5. P0-2 安全热点后续专项落地

## 执行记录

- 已完成：
  - `cargo fmt --all`
  - `cargo fmt --all -- --check`
  - `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/benchmark.yml"); puts "workflow yaml: OK"'`
  - `cargo test --doc --locked`
  - `cargo test --locked --test unit -- --test-threads=1`
  - `cargo clippy --all-features --locked -- -D warnings`
  - `git ls-files '*.key' '*.p12'`
  - `cargo test --locked test_validate_state_pkce_binding -- --nocapture`
  - `cargo test --locked test_validate_response_rejects_non_success_status -- --nocapture`
  - `cargo test --locked test_validate_response_rejects_mismatched_destination -- --nocapture`
  - `cargo test --locked test_validate_response_rejects_mismatched_recipient -- --nocapture`
  - `cargo test --locked test_ensure_local_admin_registration_request_rejects_non_local_origin -- --nocapture`
  - `cargo test --locked test_ensure_local_admin_registration_request_accepts_local_origin -- --nocapture`
- 当前保留项：
  - `api_admin` 定向集成测试仍存在既有 500 失败，需单独定位管理端测试基线稳定性
