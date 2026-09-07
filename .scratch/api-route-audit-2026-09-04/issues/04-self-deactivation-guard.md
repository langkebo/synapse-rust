# A4: /users/{user_id}/deactivate 加 self-deactivation guard

**状态**：✅ DONE
**优先级**：💭 低
**审计来源**：API 路由安全审计 #A4（2026-09-04）

## 问题描述

`src/web/routes/admin/user.rs::deactivate_user` 允许 admin 停用任意 user，包括 admin 自己。

**实际风险**：
- 单 admin 部署中，admin 停用自己会导致 homeserver 失去所有管理能力（无回退）
- 多 admin 部署中可由其他 admin 重启，但需明确警告
- 与 `set_admin`（允许 super_admin 改他人 admin）相比，deactivate 缺类似的"自我保护"语义

**修复方案**：拒绝 admin 停用自己，返回 400：

```rust
if admin.user_id == user.user_id {
    return Err(ApiError::bad_request(
        "Cannot deactivate your own admin account; ask another admin to do it".to_string(),
    ));
}
```

## 验收标准

1. [ ] `deactivate_user` 拒绝 `admin.user_id == target user_id` 的请求（返回 400）
2. [ ] admin 停用其他 admin 仍正常（不影响正常业务）
3. [ ] `cargo build --locked` ✅
4. [ ] `cargo clippy --workspace --all-targets -D warnings` 零警告
5. [ ] `cargo test --test integration api_admin_user_lifecycle_tests` PASS

## 改动范围

```
src/web/routes/admin/user.rs        # deactivate_user 函数头加 self-guard
```

## 注意事项

- 严格说，被动的 `set_admin` 也不应让 super_admin 撤销自己（应要求另一个 super_admin）。但 audit 没标，且 admin::mod.rs 的 ensure_super_admin 已隐含——暂不扩展。
- 本 guard 是防御性 UX 改进，不是安全 blocker
