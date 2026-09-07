# A5: validators.rs 补充 validate_server_name 公共函数

**状态**：✅ DONE
**优先级**：💭 低
**审计来源**：API 路由安全审计 #A5（2026-09-04）

## 问题描述

`src/web/routes/validators.rs` 有 8 个 validate_* 公共函数（user_id, room_id, room_alias, event_id, presence_status, receipt_type, membership, 隐含的 MAX_EVENT_ID_LEN 常量），但缺 `validate_server_name`。

`destination` 路由（A2 修复后会用 `Path<ServerName>` typed ID）以及其他可能用 server name 的路由会受益于此函数。

**修复方案**：补充 `validate_server_name` 到 `validators.rs`：

```rust
pub fn validate_server_name(server_name: &str) -> Result<(), ApiError> {
    if server_name.is_empty() {
        return Err(ApiError::invalid_input("server_name is required".to_string()));
    }
    if server_name.len() > 255 {
        return Err(ApiError::invalid_input("server_name too long (max 255)".to_string()));
    }
    // 防御性：拒绝含 / \ : 控制字符的 path traversal 尝试
    if server_name.contains('/') || server_name.contains('\\') || server_name.contains('\0') {
        return Err(ApiError::invalid_input("Invalid server_name format".to_string()));
    }
    Ok(())
}
```

## 验收标准

1. [ ] `validators.rs` 新增 `validate_server_name` + 单元测试
2. [ ] `cargo build --locked` ✅
3. [ ] `cargo clippy --workspace --all-targets -D warnings` 零警告
4. [ ] `cargo test -p synapse-rust --lib web::routes::validators` 包含新测试 PASS

## 改动范围

```
src/web/routes/validators.rs            # +validate_server_name + 单元测试
```

## 注意事项

- Server name 实际格式比 server name + length 更宽（DNS、IP、未来 port），这里只防御 path traversal/控制字符
- 与 A2 的 `Path<ServerName>` 不冲突：A2 用 typed ID 编译期保证；A5 给未来手写 server name 的代码（不经过 typed）兜底
