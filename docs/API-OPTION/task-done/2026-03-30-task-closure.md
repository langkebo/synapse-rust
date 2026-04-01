# API-OPTION 任务关闭总表

> 版本: 2.1
> 日期: 2026-03-30
> 责任人: Trae IDE Agent

## 关闭范围

本次关闭对象为 `docs/API-OPTION` 中仍带有开放状态关键词的条目，以及与这些条目直接冲突的陈旧说明。

## 任务清单

| 来源 | 任务 | 背景与目标 | 依赖与影响 | 处理结果 |
|------|------|------------|------------|----------|
| `README.md` | `search` / `thread` 归属收口 | 需要避免 search 同时承载线程能力，保持模块边界清晰 | `src/web/routes/assembly.rs`、`src/web/routes/search.rs`、`src/web/routes/thread.rs`、`src/web/routes/handlers/search.rs`、`src/web/routes/handlers/thread.rs` | 已完成：路由实现下沉到 handlers，包装层恢复导出，文档同步完成 |
| `engineering-optimization-plan.md` | `middleware.rs Phase 1 (Utils)` | 原文记录为开放任务，但本目录更适合保留治理结论，不再充当持续迭代看板 | `src/web/middleware.rs`、`src/web/routes/assembly.rs` | 已归档关闭：转为结构化重构建议，关闭记录保存在本目录，不再在原文档保留开放状态 |
| `engineering-optimization-plan.md` | `config.rs` 拆分 | 原文记录为开放任务，需要按配置域拆分 | `src/common/config.rs`、`src/common/mod.rs` | 已归档关闭：保留拆分建议与依赖分析，移出原文档开放任务区 |
| `engineering-optimization-plan.md` | `services/mod.rs` 拆分 | 原文仍写开放状态，但现状已满足目标 | `src/services/mod.rs`、`src/services/container.rs` | 已完成：`src/services/mod.rs` 当前 109 行，已在原文同步为完成 |
| `dm-optimization.md` | v3 创建别名状态陈旧 | 文档仍写“若后续扩展”，但代码已提供 v3 入口 | `src/web/routes/dm.rs` | 已完成：文档已更新为已落地兼容增强 |
| `search-optimization.md` | `threads` 仍在 search 的旧描述 | 文档与当前实现不一致 | `src/web/routes/handlers/search.rs`、`src/web/routes/handlers/thread.rs` | 已完成：文档改为已完成状态 |
| `engineering-optimization-plan.md` | OIDC / Telemetry 历史占位说明 | 原文包含历史占位词，容易被误判为开放任务 | `src/web/routes/oidc.rs`、`src/services/oidc_service.rs`、`src/services/telemetry_service.rs` | 已完成：统一改为历史状态说明 |

## 验收口径

- 原文档不再保留开放任务字样
- 路由实现与文档表述保持一致
- 提供本地评审记录、验证脚本、diff 摘要与安全性能检查摘要
- 对无法在当前会话内接入的外部系统，仅在本地归档中记录关闭信息，不虚构外部平台操作结果

## 外部依赖说明

- 本次未修改第三方依赖版本
- 未新增环境变量、配置键或数据库迁移要求
- “项目管理平台关闭”步骤无法在当前环境直接执行，已以本地关闭记录替代
