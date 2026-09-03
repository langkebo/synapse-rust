# 01: RateLimitConfig / LoginLockoutConfig 加 fail_open_on_lockout 配置字段

**What to build:** 在 synapse 配置结构中新增一个 `fail_open_on_login_lockout: bool` 配置项，默认 `true`（保持向后兼容）。当 Redis 不可用时，运维可设 `false` 让系统在 Redis 故障时硬拒绝登录，避免 fail-open 带来的无限暴力破解窗口。

**Blocked by:** None（无依赖，立即可开始）

**Status:** ✅ done (commit pending)

- [x] 在 `synapse-common/src/config/` 找到合适位置（security）添加字段
- [x] 默认值为 `true`，与现状一致，不破坏现有部署
- [x] `Config::validate()` 不要求该字段存在（向后兼容）
- [x] 提供 yaml 示例片段到 `docker/config/homeserver.yaml`
- [x] cargo build --locked 通过