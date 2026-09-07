# A2: destination 路由改用 ServerName typed ID

**状态**：✅ DONE
**优先级**：💭 低
**审计来源**：API 路由安全审计 #A2（2026-09-04）

## 问题描述

`src/web/routes/admin/federation.rs` 4 个路由使用 `Path<String>` 取 `destination`：
- `get_destination` (line 168)
- `reset_connection` (line 180)
- `delete_destination` (line 190)
- `get_destination_rooms` (line 200)

`destination` 是联邦对端 server name，本应是 `ServerName` typed ID 但当前退化为 `Path<String>`。

**实际风险**：极低。`destination` 受 `federation_auth_middleware` 保护，且 server 层握手会拒绝无效 server name。

**修复方案**：替换为 `Path<ServerName>`（`synapse-common::types::ServerName`）。`matrix_id!` 宏的 `FromStr` 实现保证非空 + 长度 ≤ 255（编译期/反序列化期保证）。

## 验收标准

1. [ ] 4 个 `destination` 路由签名改为 `Path<ServerName>`
2. [ ] `cargo build --locked` ✅
3. [ ] `cargo clippy --workspace --all-targets -D warnings` 零警告
4. [ ] `cargo test --test integration api_admin_federation_tests` PASS

## 改动范围

```
src/web/routes/admin/federation.rs          # 4 个 handler 改 Path<ServerName>
```

## 注意事项

- `ServerName` 的 Deref 是 `&str`，service 层参数仍是 `&str` 无需改
- `destination.rs` 的 `encode_destination_cursor`/`decode_destination_cursor` 仍接受 `&str`，无需改
