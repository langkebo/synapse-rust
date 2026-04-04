# synapse-rust 文档索引与优先级

> 目的：建立单一事实源前的文档治理基线  
> 范围：根 README、`docs/synapse-rust/`、`docs/db/` 中与项目状态、能力结论、迁移与发布口径直接相关的正式文档

---

## 一、文档优先级规则

| 级别 | 定义 | 用途 |
|------|------|------|
| P1 | 对外主口径文档 | 项目状态、部署方式、能力摘要、发布边界 |
| P2 | 正式事实源文档 | 能力矩阵、问题台账、测试接线清单、发布准入 |
| P3 | 专题分析文档 | 单一能力域分析、单轮审计、专项测试报告 |
| P4 | 历史材料 | 仅供追溯，不可直接作为当前结论 |

---

## 二、当前建议索引

### 2.1 P1 对外主口径

| 文档 | 当前角色 | 建议角色 |
|------|------|------|
| [README.md](file:///Users/ljf/Desktop/hu/synapse-rust/README.md) | 项目入口文档 | 保留为唯一对外入口，展示项目状态、部署方式、能力摘要、事实源链接 |
| [DEPLOYMENT_GUIDE.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/DEPLOYMENT_GUIDE.md) | 部署说明 | 保留为唯一正式部署指引 |
| [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 正式分析与执行方案 | 作为当前阶段总纲文档 |
| [CAPABILITY_STATUS_BASELINE_2026-04-02.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/CAPABILITY_STATUS_BASELINE_2026-04-02.md) | 正式能力状态基线 | 作为当前阶段唯一正式能力状态摘要 |

### 2.2 P2 正式事实源

| 文档 | 当前角色 | 建议角色 |
|------|------|------|
| [capability-status-baseline.md](file:///Users/ljf/Desktop/hu/synapse-rust/.trae/specs/analyze-synapse-gap-and-optimization/capability-status-baseline.md) | 新增 | 统一能力状态枚举与能力总表模板 |
| [test-execution-inventory.md](file:///Users/ljf/Desktop/hu/synapse-rust/.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md) | 新增 | 测试接线与执行事实源 |
| [remediation-backlog.md](file:///Users/ljf/Desktop/hu/synapse-rust/.trae/specs/analyze-synapse-gap-and-optimization/remediation-backlog.md) | 已建 | 可执行整改台账 |
| [document-conflicts.md](file:///Users/ljf/Desktop/hu/synapse-rust/.trae/specs/analyze-synapse-gap-and-optimization/document-conflicts.md) | 新增 | 冲突口径清单与处理建议 |

### 2.3 P3 专题分析

| 文档 | 当前角色 | 处理建议 |
|------|------|------|
| [SYNAPSE_COMPARISON.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/SYNAPSE_COMPARISON.md) | Synapse 对比报告 | 保留，但后续改为引用能力矩阵而非直接给出总完成度 |
| [API_COVERAGE_REPORT.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md) | 路由覆盖报告 | 保留，明确仅代表覆盖率，不代表行为兼容度 |
| [API_TEST_REPORT.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_TEST_REPORT.md) | 测试结果报告 | 保留为历史测试记录，需标明时间点与前提条件 |
| [E2EE_ANALYSIS.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/E2EE_ANALYSIS.md) | E2EE 专题 | 保留，后续按验证状态收敛 |
| [CORE_FEATURES_ANALYSIS.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/CORE_FEATURES_ANALYSIS.md) | 核心功能分析 | 保留，标注为阶段性分析 |
| [FEATURE_IMPROVEMENT_REPORT.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/FEATURE_IMPROVEMENT_REPORT.md) | 特性补强建议 | 保留为历史改进记录，后续仅作追溯 |

### 2.4 P4 历史材料

| 文档 | 原因 | 处理建议 |
|------|------|------|
| [COMPLETION_REPORT.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/COMPLETION_REPORT.md) | 含“100%”“生产就绪”等高结论性措辞，易与当前基线冲突 | 归档为历史结论文档，仅供追溯 |
| [MISSING_FEATURES.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/MISSING_FEATURES.md) | 需与统一能力总表对齐 | 修订后再恢复为正式引用 |
| 单轮 rerun / 分析报告 | 时间点强、环境依赖强 | 统一标记为阶段性材料 |

---

## 三、当前使用建议

1. 对外引用优先使用 README 与正式执行方案。
2. 对内判断真实状态优先使用能力总表、测试接线清单与整改台账。
3. 专题报告不得单独作为“已完成/生产可用”结论来源。
4. 历史报告必须增加“时间点、环境、适用范围”标识。
