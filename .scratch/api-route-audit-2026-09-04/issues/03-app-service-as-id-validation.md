# A3: app_service 路由加显式 ID 格式验证

**状态**：✅ DONE
**优先级**：💭 低
**审计来源**：API 路由安全审计 #A3（2026-09-04）

## 问题描述

`src/web/routes/app_service.rs` 中 12 个 `as_id` 路由（`Path<String>`）和 1 个 `alias` 路由（`Path<String>`）无显式格式验证。

**实际风险**：极低。受 `app_service_auth_middleware` 保护（AS 握手验 secret + 签名）；`as_id` 是 AS 注册时分配的 opaque string（业务逻辑侧校验）。

**修复方案**：加 2 个轻量级验证函数到 `validators.rs`，在路由 handler 入口调用：

```rust
// validators.rs
pub fn validate_as_id(as_id: &str) -> Result<(), ApiError> {
    if as_id.is_empty() {
        return Err(ApiError::invalid_input("as_id is required".to_string()));
    }
    if as_id.len() > 255 {
        return Err(ApiError::invalid_input("as_id too long (max 255)".to_string()));
    }
    // AS IDs are opaque application-specific strings (Matrix spec §13.9).
    // Only structural constraints (non-empty, bounded length) are applied.
    Ok(())
}
```

`app_service.rs` 14 个 handler 入口统一加 `validate_as_id(&as_id)?;` / `validate_as_id(&alias)?;`。

## 验收标准

1. [ ] `validators.rs` 新增 `validate_as_id` + 单元测试
2. [ ] `app_service.rs` 14 个 handler 入口调用验证
3. [ ] `cargo build --locked` ✅
4. [ ] `cargo clippy --workspace --all-targets -D warnings` 零警告

## 改动范围

```
src/web/routes/validators.rs            # +validate_as_id + 单元测试
src/web/routes/app_service.rs           # 14 个 handler 入口加 validate_as_id(&as_id)?
```
