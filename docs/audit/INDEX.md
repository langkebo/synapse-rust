# Synapse-Rust 审计文档索引

## 📋 活跃文档（当前有效）

### 核心问题追踪

| 文档 | 描述 | 更新日期 |
|------|------|----------|
| **[UNRESOLVED_ISSUES_SUMMARY.md](UNRESOLVED_ISSUES_SUMMARY.md)** | 当前仍存问题总结（权威来源） | 2026-10-04 |
| **[INDEX.md](INDEX.md)** | 本索引文档 | 2026-10-04 |
| **[COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md)** | 全面系统遗留问题排查报告 | 2026-10-04 |

### Room v12 合规

| 文档 | 描述 | 状态 |
|------|------|------|
| **[ROOM_V12_UPSTREAM_ALIGNMENT_OPTIMIZATION_2026-09-28.md](ROOM_V12_UPSTREAM_ALIGNMENT_OPTIMIZATION_2026-09-28.md)** | Room v12 上游对齐优化方案 | ✅ Active |
| **[ROOM_V12_PLAN_STATUS_2026-09-27.md](ROOM_V12_PLAN_STATUS_2026-09-27.md)** | Room v12 实施计划状态 | ✅ Active |
| **[ROOM_V12_B2A_DELIVERY_2026-09-27.md](ROOM_V12_B2A_DELIVERY_2026-09-27.md)** | B2A 交付验证 | ✅ Active |
| **[ROOM_V12_COMPLETION_PLAN_2026-09-27.md](ROOM_V12_COMPLETION_PLAN_2026-09-27.md)** | 完成计划 | ✅ Active |
| **[O1_PHASE1_V12_IMPLEMENTATION_DETAILS.md](O1_PHASE1_V12_IMPLEMENTATION_DETAILS.md)** | v12 实施细节 | ✅ Active |

### 联邦与互操作

| 文档 | 描述 | 状态 |
|------|------|------|
| **[A5_LIVE_FEDERATION_INTEROP_TESTING.md](A5_LIVE_FEDERATION_INTEROP_TESTING.md)** | A5 联邦互操作测试 | ✅ Verified (2026-09-28) |
| **[v12-pdu-graph-fields-integration.md](v12-pdu-graph-fields-integration.md)** | PDU 图字段集成 | ✅ Active |
| **[MUTATION_6_FAILURE_ANALYSIS_2026-09-29.md](MUTATION_6_FAILURE_ANALYSIS_2026-09-29.md)** | Mutation 测试失败分析 | ✅ Active |
| **[MUTATION_6_REDESIGN_SYNTHETIC_2026-09-29.md](MUTATION_6_REDESIGN_SYNTHETIC_2026-09-29.md)** | Mutation 重新设计 | ✅ Active |

### MSC 与实验性功能

| 文档 | 描述 | 状态 |
|------|------|------|
| **[sdk-encapsulation-audit.md](sdk-encapsulation-audit.md)** | SDK 封装审计 | ✅ Active |
| **[CAS_ROUTER_PREFIX_MISSING_2026-09-29.md](CAS_ROUTER_PREFIX_MISSING_2026-09-29.md)** | CAS 路由器前缀缺失 | ✅ Active |

### 其他规划文档

| 文档 | 描述 | 状态 |
|------|------|------|
| **[REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md](REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md)** | 剩余问题验证与优化计划 | ✅ Active |
| **[ISSUE_REVIEW_2026-09-28.md](ISSUE_REVIEW_2026-09-28.md)** | 问题审查总结 | ✅ Active |
| **[SQLX_STATICIZATION_PLAN_2026-09-23.md](SQLX_STATICIZATION_PLAN_2026-09-23.md)** | SQLX 静态化计划 | ✅ Active |
| **[DB_REVIEW_2026-09-17.md](DB_REVIEW_2026-09-17.md)** | 数据库审查 | ✅ Active |
| **[D-12_EVENT_REPORT_HISTORY_STATS_FIX_PLAN.md](D-12_EVENT_REPORT_HISTORY_STATS_FIX_PLAN.md)** | 事件报告历史统计修复计划 | ✅ Active |

### 技术策略

| 文档 | 描述 | 状态 |
|------|------|------|
| **[A3_v1_deprecation_strategy.md](A3_v1_deprecation_strategy.md)** | v1 弃用策略 | ✅ Active |
| **[P5_engineering_2026-09-11.md](P5_engineering_2026-09-11.md)** | 工程问题分类 | ✅ Active |

---

## 📦 归档文档（已过时/已解决）

以下文档已移动至 `archive/` 子目录，供历史参考：

**归档数量**: 41 个文件

主要归档类别：
- P1-P5 性能基准与基础设施问题
- Gate 完整性验证系列
- 部署与验证流程
- 早期架构补救路线图
- 缓存读写审计
- SIGTERM 根因分析
- 测试隔离模板与设计

👉 **访问方式**: `docs/audit/archive/` 目录

---

## 📊 问题状态摘要

### 已解决的问题（2026-10-04 更新）

| 问题 | 优先级 | 解决日期 | 解决方案 |
|------|--------|----------|----------|
| 联邦 `/send_join` PDU 语义 | P0 | 2026-09-28 | GraphMetadataWriter 接入 reference hash |
| Event ID 语义不匹配 | P0 | 2026-09-28 | Placeholder + legacy ID 再生成 |
| Soft_failed 读路径过滤 | P0 | 2026-09-28 | 全面覆盖客户端读取面 |
| 客户端撤回级联 | P2 | 2026-09-28 | 完整实现 MSC3912 |
| Animated 缩略图支持 | P2 | 2026-09-28 | Phase 2 全功能实现 |
| Content Scanner 误判 | P2 | 2026-10-04 | 验证为已集成，原审计误判 |
| Admin 媒体端点族补齐 | P3 | 2026-10-04 | `purge_media_cache` 路由已添加并完成契约更新 |

### 当前待处理问题

| 问题 | 优先级 | 状态 | 负责人 |
|------|--------|------|--------|
| Profile 接口差异 | P3 | 🔴 待处理 | - |
| Ledger query_params 未使用 | P3 | 🔴 待处理 | - |
| Search_index 遗留表 | P3 | 🟡 低优先级 | - |
| v12/v13 房间创建决策 | P4 | 🟡 待决策 | - |
| MSC4242/4502/4262 语义对齐 | P4 | 🟡 部分完成 | - |

---

## 🔍 文档使用指南

### 快速查阅

1. **了解当前问题**: 阅读 [UNRESOLVED_ISSUES_SUMMARY.md](UNRESOLVED_ISSUES_SUMMARY.md)
2. **查看整体报告**: 阅读 [COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md)
3. **追踪 Room v12 进度**: 查看 `ROOM_V12_*` 系列文档
4. **联邦互操作验证**: 查看 `A5_LIVE_FEDERATION_INTEROP_TESTING.md`

### 深入分析

- **Mutation 测试方法论**: `MUTATION_6_*` 系列
- **数据库 Schema 变更**: `DB_REVIEW_2026-09-17.md`, `SQLX_STATICIZATION_PLAN_2026-09-23.md`
- **实验性功能语义**: `sdk-encapsulation-audit.md`

### 历史记录

- 归档文档位于 `archive/` 目录
- 如需查找特定日期的审计报告，请查看归档目录

---

## 📅 更新日志

### 2026-10-04

- ✅ 创建此索引文档
- ✅ 归档 41 个过时报告到 `archive/`
- ✅ 更新 `UNRESOLVED_ISSUES_SUMMARY.md`：
  - 客户端撤回级联验证为已实现
  - Animated 缩略图验证为已实现
  - Content Scanner 标记为误判
- ✅ 补齐 Admin 媒体端点族：
  - `purge_media_cache` 路由已添加
  - 契约产物同步更新

### 2026-09-28

- ✅ P0 问题全部解决（PDU 语义、Event ID、Soft_failed）
- ✅ A5 联邦互操作测试完成
- ✅ MSC3912 客户端撤回级联实现
- ✅ 动画缩略图 Phase 2 完成

---

## 📞 联系与反馈

如有问题或需要更新此索引，请联系项目维护团队。

**文档维护者**: Audit Team  
**最后审核**: 2026-10-04  
**下次计划更新**: 2026-10-11