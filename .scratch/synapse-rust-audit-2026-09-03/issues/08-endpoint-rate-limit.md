# 08: homeserver.yaml 为关键端点配置差异化严格限流（P2-1）

**What to build:** 在 `docker/config/homeserver.yaml` 中为 `/login` 和 `/register` 配置比普通端点更严格的限流参数，防范低复杂度 DoS。编辑 `rate_limit.endpoints` 数组，添加针对关键认证端点的覆盖配置。

**Blocked by:** None（无依赖，立即可开始）

**Status:** ✅ done（cargo build --tests ✅，Python YAML 验证 ✅）

- [x] 在 `rate_limit.endpoints` 中添加 `/login` 端点：`per_second: 5, burst_size: 3`
- [x] 添加 `/register` 端点：`per_second: 5, burst_size: 3`
- [x] 保留默认全局限流不变（`per_second: 20, burst_size: 40`）
- [x] 更新 `homeserver.yaml` 模板中的注释说明
- [x] cargo build --locked --tests 通过
- [x] Python YAML 验证：4 个端点正确解析

## 验证

**Python YAML 解析**：✅ homeserver.yaml 结构正确，4 个端点配置正确加载

**cargo build（SQLX_OFFLINE=true）**：✅ synapse-common rate_limit 27 tests ✅，synapse-rust 全量 test targets 编译通过

## 修改文件

| 文件 | 改动 |
|------|------|
| `docker/config/homeserver.yaml` | `rate_limit.endpoints` 新增 4 个规则：`/_matrix/client/v3/login`、`/_matrix/client/r0/login`、`/_matrix/client/v3/register`、`/_matrix/client/r0/register`，均 `match_type: exact`，`per_second: 5, burst_size: 3` |
| `synapse-common/src/config/mod.rs` | 补 4 处 `SecurityConfig` literal 缺失字段 |
| `synapse-common/src/argon2_config.rs` | 补 2 处 `SecurityConfig` literal 缺失字段 |
| `src/common/config/tests.rs` | 补 5 处 `SecurityConfig` literal 缺失字段 |