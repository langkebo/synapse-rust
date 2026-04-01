# synapse-rust 文档冲突清单

> 目的：定位当前最影响项目判断准确性的文档口径冲突  
> 状态枚举：待处理 / 已收敛 / 已归档  
> 建议优先级：P0 / P1 / P2

---

## 一、冲突清单

| ID | 优先级 | 冲突主题 | 冲突描述 | 证据 | 建议处理 | 状态 |
|------|------|------|------|------|------|------|
| DOC-01 | P0 | 项目总体状态 | README 已完成首轮修正，但历史总结报告仍存在“100% 完成、生产就绪”等过度结论 | [README.md:L3-L11](file:///Users/ljf/Desktop/hu/synapse-rust/README.md#L3-L11) / [COMPLETION_REPORT.md:L66-L76](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/COMPLETION_REPORT.md#L66-L76) / [CORE_FEATURES_ANALYSIS.md:L7-L18](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/CORE_FEATURES_ANALYSIS.md#L7-L18) | 以统一能力总表替代笼统状态词；继续收敛历史结论文档 | 待处理 |
| DOC-02 | P0 | Client API 完成度 | README 已不再使用“部分实现”表述，但其他文档仍使用 API 100% 或接近全覆盖的高结论口径 | [README.md:L5-L11](file:///Users/ljf/Desktop/hu/synapse-rust/README.md#L5-L11) / [COMPLETION_REPORT.md:L68-L75](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/COMPLETION_REPORT.md#L68-L75) / [API_COVERAGE_REPORT.md:L5-L18](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md#L5-L18) | 拆分为“路由覆盖率 / 行为验证完成度 / 协议兼容度”三类指标 | 待处理 |
| DOC-03 | P0 | 数据库迁移入口 | README、运行时代码与部署指南已统一到“外部迁移脚本为唯一推荐入口”的口径 | [README.md:L49-L59](file:///Users/ljf/Desktop/hu/synapse-rust/README.md#L49-L59) / [server.rs:L115-L128](file:///Users/ljf/Desktop/hu/synapse-rust/src/server.rs#L115-L128) / [DEPLOYMENT_GUIDE.md:L198-L203](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/DEPLOYMENT_GUIDE.md#L198-L203) | 保持一致，不再新增绕过事实源的迁移表述 | 已收敛 |
| DOC-04 | P1 | E2EE 成熟度 | 专题文档与完成度报告使用“完整实现、生产就绪”，未完成清单仍显示多项仅部分实现或待验证 | [E2EE_ANALYSIS.md:L7-L25](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/E2EE_ANALYSIS.md#L7-L25) / [COMPLETION_REPORT.md:L20-L37](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/COMPLETION_REPORT.md#L20-L37) / [TODO-UNFINISHED-TASKS.md:L47-L56](file:///Users/ljf/Desktop/hu/synapse-rust/docs/TODO-UNFINISHED-TASKS.md#L47-L56) | 改成分项状态：路由、基础链路、跨设备、恢复、交叉签名分别表述 | 待处理 |
| DOC-05 | P1 | Federation 成熟度 | 完成度报告给出高成熟度结论，但专题与测试材料仍列出待验证或未完成项 | [COMPLETION_REPORT.md:L38-L54](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/COMPLETION_REPORT.md#L38-L54) / [FEATURE_IMPROVEMENT_REPORT.md:L78-L86](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/FEATURE_IMPROVEMENT_REPORT.md#L78-L86) / [TODO-UNFINISHED-TASKS.md:L36-L46](file:///Users/ljf/Desktop/hu/synapse-rust/docs/TODO-UNFINISHED-TASKS.md#L36-L46) | 拆成路由覆盖、签名配置、跨 homeserver 互通、管理扩展四层状态 | 待处理 |
| DOC-06 | P1 | AppService 定位 | 有文档写“应用服务完善”，也有文档写“部分实现/未完全实现” | [COMPLETION_REPORT.md:L55-L63](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/COMPLETION_REPORT.md#L55-L63) / [FEATURE_IMPROVEMENT_REPORT.md:L51-L66](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/FEATURE_IMPROVEMENT_REPORT.md#L51-L66) / [API_TEST_REPORT.md:L311-L321](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_TEST_REPORT.md#L311-L321) | 明确当前是“路由/管理面存在”还是“完整 AS 行为可用” | 待处理 |
| DOC-07 | P1 | Worker 定位 | 对比报告容易让人理解为 Worker 已存在即可用，但未完成清单显示复制协议、队列仍未完成 | [SYNAPSE_COMPARISON.md:L60-L73](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/SYNAPSE_COMPARISON.md#L60-L73) / [TODO-UNFINISHED-TASKS.md:L122-L130](file:///Users/ljf/Desktop/hu/synapse-rust/docs/TODO-UNFINISHED-TASKS.md#L122-L130) | 明确当前正式定位为“单进程可用，多 Worker 未成熟” | 待处理 |
| DOC-08 | P1 | 同一报告内部自冲突 | 同一测试报告前文写部分实现，后文汇总又写全部已实现 | [API_TEST_REPORT.md:L280-L295](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_TEST_REPORT.md#L280-L295) / [API_TEST_REPORT.md:L338-L349](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_TEST_REPORT.md#L338-L349) | 汇总表改为从细项状态自动生成或统一引用前文结论 | 待处理 |

---

## 二、处理顺序

1. 先处理 README 与迁移入口冲突。
2. 再处理项目总体状态、Client API、E2EE、Federation 的主结论冲突。
3. 最后收敛 AppService、Worker 与单篇报告内部自冲突问题。

---

## 三、处理原则

1. 禁止继续使用“100%”“完整实现”“生产就绪”这类无证据约束的表述。
2. 所有结论都要回链到统一能力总表。
3. 专题报告只保留专题结论，不再承担全局成熟度结论职责。
4. 历史报告不删除，但必须降低优先级并标注适用范围。
