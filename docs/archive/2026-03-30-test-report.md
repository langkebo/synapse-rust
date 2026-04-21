# API-OPTION 验证与回归记录

> 日期: 2026-03-30
> 版本: 2.1
> 责任人: Trae IDE Agent

## 已执行验证

| 命令 | 结果 |
|------|------|
| `cargo check --all-targets` | 通过 |
| `cargo test search_routes` | 通过 |
| `cargo test legacy_thread` | 通过 |
| `cargo test parse_dm_users_requires_string_array` | 通过 |
| 顶层 API-OPTION 文档开放状态关键词扫描 | 无匹配 |

## 验证说明

- `cargo check --all-targets` 确认路由包装层恢复后编译正常
- `cargo test search_routes` 覆盖 search 路由结构与“search 不再宣称 thread 兼容入口”
- `cargo test legacy_thread` 覆盖 thread 兼容响应结构
- `cargo test parse_dm_users_requires_string_array` 用于确认 DM 路由相关单元测试仍可通过

## 未纳入本轮成功基线的项

- `cargo fmt --all -- --check` 未作为通过项写入脚本，因为工作区中存在本次交付范围之外的未格式化改动
- 未执行全量 `cargo test --all-features`，原因是本轮改动仅涉及路由包装与文档同步，优先执行了与改动直接相关的编译与单测
