# Tasks

- [x] Task 1: 建立对标基线与分析边界
  - [x] 汇总 Matrix 规范范围、Synapse 上游项目形态与本仓库目标边界
  - [x] 定义能力状态枚举：已实现并验证、已实现待验证、部分实现、未实现、不纳入本期
  - [x] 明确标准能力、Synapse 兼容能力与项目私有扩展的边界

- [x] Task 2: 输出系统性问题总表
  - [x] 从架构、兼容性、测试、文档、部署、运维、安全六个维度提取问题
  - [x] 为每个问题补充证据、影响范围、根因与优先级
  - [x] 建立 P0/P1/P2/P3 问题台账并定义处置顺序

- [x] Task 3: 建立兼容性差距矩阵
  - [x] 建立 Matrix 标准域矩阵：Client-Server、Server-Server、Application Service、Identity、Push、E2EE、Room Versions、稳定 MSC
  - [x] 建立 Synapse 关键能力矩阵：Admin API、Worker、联邦、SSO/OIDC/SAML、媒体、后台任务、部署治理
  - [x] 将每一项能力映射到本仓库代码、测试、文档与现状结论

- [x] Task 4: 制定 P0 止血方案
  - [x] 统一 README、完成度报告、缺失功能与测试结论的状态口径
  - [x] 识别并修复测试接线缺失、弱断言、跳过项掩盖风险等问题
  - [x] 统一数据库初始化、迁移入口、发布准出与回滚说明

- [x] Task 5: 制定核心能力收敛方案
  - [x] 为联邦、E2EE、Admin、SSO、AppService、Worker 六大关键域定义最小闭环目标
  - [x] 为每个关键域定义实现缺口、验证缺口、文档缺口与依赖项
  - [x] 为每个关键域定义阶段性交付物、验收标准与回归测试要求

- [x] Task 6: 制定架构收敛方案
  - [x] 评估 `ServiceContainer`、总路由装配、单体服务边界与 worker 边界的耦合风险
  - [x] 设计分层拆分策略、模块收口策略与依赖治理策略
  - [x] 定义短期可控重构与长期架构演进的优先级

- [x] Task 7: 制定验证与质量门禁方案
  - [x] 建立单元、集成、联邦、互操作、性能与稳定性测试的执行矩阵
  - [x] 明确所有测试入口、CI 接线、产物位置与失败阻断规则
  - [x] 设计“已实现”升级为“已验证”的证据规则

- [x] Task 8: 制定单一事实源文档治理方案
  - [x] 设计统一状态总表与文档来源优先级
  - [x] 定义旧报告归档、历史结论标注与自动更新策略
  - [x] 明确外部可读文档与内部执行文档的职责分层

- [x] Task 9: 制定分阶段优化路线图
  - [x] 输出 M0 基线清点、M1 P0 止血、M2 核心能力收敛、M3 质量门禁、M4 架构演进五阶段路线图
  - [x] 为每阶段定义输入、输出、负责人角色、依赖关系、风险与回滚策略
  - [x] 为每阶段定义明确的退出条件与发布准入门槛

- [x] Task 10: 形成最终交付包
  - [x] 汇总差距分析报告、兼容矩阵、问题台账、优化路线图与门禁方案
  - [x] 校验方案与仓库目录、测试体系、迁移规范、发布方式一致
  - [x] 输出可执行的实施建议，作为后续整改工作的统一入口

- [ ] Task 11: 建立空壳接口债务治理方案
  - [ ] 盘点 `src/web/routes/` 中“已鉴权/已校验但返回静态占位结果”的接口模式
  - [ ] 输出接口清单、复用的 `service/storage` 路径、整改优先级与风险评估
  - [ ] 设计自动扫描规则与 CI 阻断策略，定义豁免机制与清理时限

- [ ] Task 12: 制定房间域与核心路由拆分方案
  - [ ] 评估 `room.rs`、`middleware.rs`、`e2ee_routes.rs` 的职责聚合与维护风险
  - [ ] 输出按领域拆分的模块方案，如 `room_account_data`、`room_keys`、`room_spaces`、`room_threads`、`room_render`
  - [ ] 制定拆分顺序、路由迁移策略、回归验证要求与风险回滚方案

- [ ] Task 13: 制定统一权限守卫与服务聚合收敛方案
  - [ ] 盘点房间相关接口中重复的 `room_exists`、成员校验、管理员校验组合
  - [ ] 设计统一 guard/extractor 模型，明确 403/404 错误语义、审计日志与复用入口
  - [ ] 设计 `ServiceContainer` 向 bounded context 服务聚合演进的分阶段路径

- [ ] Task 14: 制定搜索链路统一与性能优化方案
  - [ ] 盘点当前路由直查、搜索服务、索引初始化与潜在 provider 扩展点
  - [ ] 设计查询 DSL、provider 抽象与 Postgres FTS / Elasticsearch 兼容接口
  - [ ] 定义索引策略、字段规范化规则、排序/分页/过滤/highlight 一致语义与性能基线

- [ ] Task 15: 制定 schema contract test 与 migration gate 方案
  - [ ] 梳理 E2EE、account_data、space、search 等依赖 schema 的关键表与关键查询
  - [ ] 设计“迁移文件 -> 实际 schema -> SQLx 查询 -> 集成测试”的闭环校验链
  - [ ] 定义 migration gate、schema contract test、失败分类与阻断规则

- [ ] Task 16: 制定测试体系与工作区产物治理方案
  - [ ] 设计占位接口探测测试、schema 回归测试、路由契约测试三类自动化测试基线
  - [ ] 设计按能力域拆分超大测试文件的目录与命名规范
  - [ ] 设计 `docs`、`migrations`、`artifacts`、`test-results`、临时报表的分层治理与 CI 产物策略

# Task Dependencies

- Task 2 依赖 Task 1 完成对标边界与状态枚举定义
- Task 3 依赖 Task 1，且为 Task 4、Task 5 的输入
- Task 4 与 Task 6 可在 Task 2、Task 3 完成后并行推进
- Task 5 依赖 Task 3 的兼容矩阵与 Task 2 的优先级台账
- Task 7 依赖 Task 3、Task 4、Task 5 的收敛结果
- Task 8 依赖 Task 2、Task 3，确保文档治理与事实源一致
- Task 9 依赖 Task 4、Task 5、Task 6、Task 7、Task 8 的输出
- Task 10 依赖全部前置任务完成后统一编排与校验
- Task 11 依赖 Task 2、Task 5、Task 7，确保空壳接口治理与能力矩阵、验证策略一致
- Task 12 依赖 Task 6，且可与 Task 11 并行推进
- Task 13 依赖 Task 6、Task 12，用于统一权限与服务边界收敛
- Task 14 依赖 Task 3、Task 5，且与 Task 15 在方案设计阶段可并行推进
- Task 15 依赖 Task 4、Task 7、Task 14 的输入，用于形成数据库与验证闭环
- Task 16 依赖 Task 7、Task 11、Task 15，并为后续长期治理提供门禁与组织规则
