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

- [x] Task 11: 建立空壳接口债务治理方案
  - [x] 盘点 `src/web/routes/` 中“已鉴权/已校验但返回静态占位结果”的接口模式
    - 完成判定：满足下文 `Task 11 详细说明` 中的 `执行步骤` 1-3、`技术要求` 前 3 条、`质量指标` 前 3 条，并达到 `验收标准` 第 1 条。
    - 当前对应产物：`task11_room_rs_placeholder_inventory.md`、`task11_other_routes_placeholder_inventory.md`、`docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`
    - 核对结论：已满足。`room.rs` 与非 `room.rs` 清单均已给出端点、风险等级、当前行为、建议方向，并通过 `UNSUPPORTED_ENDPOINTS.md` 明确“空壳清单 / 明确不支持清单”的边界。
  - [x] 输出接口清单、复用的 `service/storage` 路径、整改优先级与风险评估
    - 完成判定：满足下文 `Task 11 详细说明` 中的 `执行步骤` 2-3、`技术要求` 第 3 条，并达到 `验收标准` 第 1 条与第 3 条。
    - 当前对应产物：`task11_room_rs_placeholder_inventory.md`、`task11_other_routes_placeholder_inventory.md`
    - 核对结论：已满足。两份 inventory 已补齐最小复用路径、整改优先级和风险说明；`M_UNRECOGNIZED` 分流规则也已固化。
  - [x] 设计自动扫描规则与 CI 阻断策略，定义豁免机制与清理时限
    - 完成判定：满足下文 `Task 11 详细说明` 中的 `执行步骤` 4-5、`技术要求` 第 4 条、全部 `测试验证方法`，并达到 `验收标准` 第 2-4 条。
    - 当前对应产物：`task11_scan_and_ci_gate.md`、`task11_placeholder_exemptions.md`
    - 核对结论：已满足。当前已完成 v1 策略草案、`handlers` 目录的 `placeholder_scan_tests.rs` 门禁，并补齐了 `task11_placeholder_exemptions.md` 模板；`tests/integration/api_placeholder_contract_p0_tests.rs` 已覆盖 `get_room_by_alias`、`get_account_data`、`get_push_rules_scope`、`room_key_distribution`、`report_room`、`get_events` 六个 P0 端点；其中 `report_room` 已归档为显式不支持，`get_events` 已验证服务报错不会回退为空 `chunk` 成功体，可勾选完成。
    - 待办清单：`task11_p0_contract_test_backlog.md`
  - [x] 已补充顶层勾选项的完成判定，并映射到 `Task 11 详细说明`
  - [x] 已完成 `handlers/room.rs` 初扫与优先级清单：`task11_room_rs_placeholder_inventory.md`
  - [x] 已完成 `src/web/routes/`（非 room.rs）初扫清单：`task11_other_routes_placeholder_inventory.md`
  - [x] 已输出 v1 扫描与 CI 阻断策略草案：`task11_scan_and_ci_gate.md`
  - [x] 已完成 `handlers/sync.rs`、`e2ee_routes.rs`、`room_summary.rs` 二轮复扫，并补录增量结论到 `task11_other_routes_placeholder_inventory.md`
  - [x] 已明确“改为 `M_UNRECOGNIZED` 的端点不再留在空壳库存中”，统一归档到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`

- [x] Task 12: 制定房间域与核心路由拆分方案
  - [x] 评估 `room.rs`、`middleware.rs`、`e2ee_routes.rs` 的职责聚合与维护风险
  - [x] 输出按领域拆分的模块方案，如 `room_account_data`、`room_keys`、`room_spaces`、`room_threads`、`room_render`
  - [x] 制定拆分顺序、路由迁移策略、回归验证要求与风险回滚方案
  - [x] 已拆出独立执行文档：`task12_room_domain_split_execution_plan.md`

- [x] Task 13: 制定统一权限守卫与服务聚合收敛方案
  - [x] 盘点房间相关接口中重复的 `room_exists`、成员校验、管理员校验组合
  - [x] 设计统一 guard/extractor 模型，明确 403/404 错误语义、审计日志与复用入口
  - [x] 设计 `ServiceContainer` 向 bounded context 服务聚合演进的分阶段路径
  - [x] 已拆出独立执行文档：`task13_room_guard_and_service_aggregation_execution_plan.md`

- [x] Task 14: 制定搜索链路统一与性能优化方案
  - [x] 盘点当前路由直查、搜索服务、索引初始化与潜在 provider 扩展点
  - [x] 设计查询 DSL、provider 抽象与 Postgres FTS / Elasticsearch 兼容接口
  - [x] 定义索引策略、字段规范化规则、排序/分页/过滤/highlight 一致语义与性能基线
  - [x] 已拆出独立执行文档：`task14_search_unification_and_performance_execution_plan.md`

- [x] Task 15: 制定 schema contract test 与 migration gate 方案
  - [x] 梳理 E2EE、account_data、space、search 等依赖 schema 的关键表与关键查询
  - [x] 设计“迁移文件 -> 实际 schema -> SQLx 查询 -> 集成测试”的闭环校验链
  - [x] 定义 migration gate、schema contract test、失败分类与阻断规则
  - [x] 已拆出独立执行文档：`task15_schema_contract_and_migration_gate_execution_plan.md`

- [x] Task 16: 制定测试体系与工作区产物治理方案
  - [x] 设计占位接口探测测试、schema 回归测试、路由契约测试三类自动化测试基线
  - [x] 设计按能力域拆分超大测试文件的目录与命名规范
  - [x] 设计 `docs`、`migrations`、`artifacts`、`test-results`、临时报表的分层治理与 CI 产物策略
  - [x] 已拆出独立执行文档：`task16_test_and_artifact_governance_execution_plan.md`

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

# Detailed Task Plans

以下详细说明用于补充 Task 11-16 的执行边界、验收口径与交付要求。状态判断以本文件顶部任务清单为准；本节用于指导执行、评审与验收。

## Task 11 详细说明：建立空壳接口债务治理方案

### 任务描述
- 目标：把 `src/web/routes/` 中“已鉴权/已校验但返回静态占位结果”的接口治理为可追踪、可验证、可阻断的工程债务对象。
- 范围：覆盖 `handlers/`、主路由模块、兼容路径与内部管理路由；不包含已经明确改为 `M_UNRECOGNIZED` 并归档到“不支持清单”的端点。
- 核心产出：空壳接口总表、整改优先级、复用路径建议、扫描规则、CI 阻断策略、豁免机制与清理时限。

### 执行步骤
1. 汇总现有盘点结果，合并 `handlers/room.rs`、其它 routes 清单、`UNSUPPORTED_ENDPOINTS.md` 的边界定义。
2. 对 `src/web/routes/` 进行全量复扫，按 `P0/P1/P2` 重新归类，标明“真实 ACK / 空壳成功 / 明确不支持”三种状态。
3. 为每个空壳端点补充最小复用路径，明确应接入的 `service/storage`、错误语义与短期兜底方案。
4. 将高风险端点转化为扫描规则和契约测试样例，形成最小 CI 门禁集。
5. 建立豁免清单模板，要求包含原因、owner、到期时间、替代方案与退出条件。

### 技术要求
- 必须区分“真实写入 ACK”与“静态假成功”，不得仅凭返回 `{}` 或空数组直接判定。
- 必须复用现有错误语义：优先使用 `M_UNRECOGNIZED`、`M_NOT_FOUND`、`M_INVALID_PARAM` 等 Matrix 标准错误。
- 盘点表必须包含端点、函数、文件、风险等级、当前行为、建议改造方向、相关 `service/storage` 证据。
- 扫描规则必须同时覆盖静态模式和行为模式，避免只靠字符串匹配造成误报。

### 完成时限
- 第一阶段（1 个工作日）：完成全量盘点收口、清单去重与优先级复核。
- 第二阶段（2 个工作日）：完成扫描规则草案、CI 阻断清单、豁免机制与时限定义。
- 第三阶段（1 个工作日）：完成评审修订并固化为后续整改入口文档。

### 质量指标
- `src/web/routes/` 目录覆盖率达到 100%，不存在明显遗漏的高风险路由文件。
- P0/P1 项必须全部具备明确整改建议，不允许只列现象不列路径。
- “空壳清单”与“明确不支持清单”零重复、零冲突。
- 每条豁免项都必须带到期时间，禁止无限期保留。

### 测试验证方法
- 文档核对：人工抽样复查不少于 10 个端点，确认分类正确。
- 静态验证：对已知占位模式运行 Grep/规则扫描，确认能命中当前样例。
- 契约验证：为 P0 端点补最小集成测试，断言“真实数据或明确错误”，禁止 200 假成功。
- 回归验证：修改清单后检查 `UNSUPPORTED_ENDPOINTS.md` 与 Task 11 文档口径一致。

### 验收标准
- 已形成统一空壳债务总表，覆盖范围、分类规则、优先级与边界定义清晰。
- 已形成 CI 可落地的最小规则集，且能说明误报控制方式。
- 已建立“不支持清单”与“空壳清单”的分流规则，并在文档中固化。
- 至少一个 P0 端点已具备可执行的契约测试设计说明。

### 预期交付物
- `task11_room_rs_placeholder_inventory.md`
- `task11_other_routes_placeholder_inventory.md`
- `task11_scan_and_ci_gate.md`
- `task11_placeholder_exemptions.md`（如开始启用豁免机制则必须补齐）
- `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`

## Task 12 详细说明：制定房间域与核心路由拆分方案

### 任务描述
- 目标：针对 `room.rs`、`middleware.rs`、`e2ee_routes.rs` 等过重文件，建立按领域拆分的模块方案，降低维护成本并收敛依赖边界。
- 范围：优先覆盖房间域、E2EE 路由、共享中间件与装配层，兼顾对现有测试与路由语义的稳定性要求。
- 核心产出：模块边界图、拆分顺序、迁移策略、风险评估、回滚方案与验证要求。

### 执行步骤
1. 盘点超大文件的路由职责、依赖关系、共享辅助函数与重复逻辑，形成当前状态快照。
2. 按业务子域设计目标模块，例如 `room_account_data`、`room_keys`、`room_spaces`、`room_threads`、`room_render`、`room_sync`。
3. 识别必须保留在聚合层的公共能力，如共享 extractor、错误映射、Router 装配入口。
4. 设计渐进拆分顺序，明确“先抽 helper / 再抽 handler / 最后收敛装配”的迁移路径。
5. 定义拆分后的验证清单，包括路由不回退、错误语义一致、测试入口不失联、文件规模下降目标。

### 技术要求
- 拆分方案必须以 bounded context 为边界，不得只做机械性按文件行数切块。
- 聚合入口必须保持稳定，避免大规模改动 `assembly.rs` 的对外行为。
- 必须明确模块命名、目录落点、共享代码放置位置与依赖方向。
- 需要说明 `middleware.rs` 中哪些逻辑应演进为 guard/extractor，哪些仍保留为横切关注点。

### 完成时限
- 第一阶段（2 个工作日）：完成现状盘点、聚合点识别与候选模块划分。
- 第二阶段（2 个工作日）：完成拆分顺序、迁移策略、风险与回滚设计。
- 第三阶段（1 个工作日）：完成评审稿与实施建议定稿。

### 质量指标
- 目标模块覆盖 `room.rs` 主要职责的 90% 以上，避免出现大量“其它杂项模块”。
- 每个新模块都必须说明归属职责、输入输出和外部依赖。
- 拆分方案需让核心文件复杂度显著下降，并给出目标行数或职责数量上限。
- 不允许引入循环依赖或多处重复装配同一路由的设计。

### 测试验证方法
- 结构验证：使用目录映射表核对“旧函数 -> 新模块”是否一一归属。
- 路由验证：抽样检查关键路径在拆分后 URL、方法、错误码保持不变。
- 依赖验证：复核 `AppState`/`ServiceContainer` 依赖是否更清晰，未新增跨域耦合。
- 回归验证：列出拆分后必须执行的单元测试、集成测试与 smoke test 清单。

### 验收标准
- 已形成可执行的模块拆分蓝图，而不是仅有方向性建议。
- 已定义清晰的迁移顺序与每一步的风险控制措施。
- 已给出拆分完成后的回归验证要求和回滚路径。
- 已明确 `room.rs`、`middleware.rs`、`e2ee_routes.rs` 的职责边界收敛方案。

### 预期交付物
- `task12_room_domain_split_plan.md`
- `task12_route_migration_matrix.md`
- `task12_validation_and_rollback.md`

## Task 13 详细说明：制定统一权限守卫与服务聚合收敛方案

### 任务描述
- 目标：统一房间相关接口中的存在性、成员身份、管理员权限等重复校验，降低复制式逻辑和错误语义漂移。
- 范围：覆盖房间主链、space、threads、E2EE 房间接口及共享路由辅助逻辑，同时评估 `ServiceContainer` 向领域服务聚合演进的路径。
- 核心产出：guard/extractor 模型、错误语义规范、审计要求、服务聚合蓝图与迁移阶段划分。

### 执行步骤
1. 盘点房间相关路由中重复出现的 `room_exists`、`is_member`、`is_admin`、creator/owner 校验组合。
2. 提炼可复用的 guard 模型，如“房间必须存在”“仅成员”“成员或管理员”“仅拥有者”。
3. 为每种 guard 定义统一的 403/404 语义、日志字段、审计点与异常处理策略。
4. 同步评估 `ServiceContainer` 的依赖分组方式，提出 `RoomServices`、`E2eeServices` 等聚合接口草案。
5. 设计逐步替换策略，明确从 helper 抽取到 extractor 落地的实施顺序与兼容要求。

### 技术要求
- guard/extractor 必须避免引入额外数据库往返或显著增加热路径延迟。
- 错误码与错误文案必须统一，可映射 Matrix 语义，不得同类场景返回不同状态。
- 服务聚合方案必须说明构造方式、生命周期、测试替身（mock/fake）可行性。
- 必须保留可审计字段，如 `user_id`、`room_id`、校验类型、拒绝原因。

### 完成时限
- 第一阶段（2 个工作日）：完成重复校验矩阵与 guard 分类。
- 第二阶段（2 个工作日）：完成 guard/extractor 设计、错误语义规范、服务聚合草案。
- 第三阶段（1 个工作日）：完成迁移分阶段计划与评审定稿。

### 质量指标
- 高频权限校验场景覆盖率达到 90% 以上。
- 同类访问控制场景的错误语义必须唯一，不能出现同条件下多种错误码。
- 方案必须明确可迁移的第一批路由，不允许停留在抽象层面。
- 服务聚合设计需降低构造复杂度，并对未来拆分具备延展性。

### 测试验证方法
- 样例验证：抽取不少于 5 组典型权限路径，验证 guard 设计可覆盖。
- 语义验证：为每个 guard 列出成功、拒绝、对象不存在三类案例。
- 集成验证：定义迁移后需要补充的权限测试与回归测试清单。
- 架构验证：评估聚合后服务构造与测试注入是否比现状更简单。

### 验收标准
- 已形成统一 guard/extractor 目录与职责模型。
- 已形成房间访问错误语义规范并给出落地路径。
- 已形成 `ServiceContainer` 向领域聚合演进的分阶段方案。
- 已明确首批试点接口与回归验证要求。

### 预期交付物
- `task13_room_guard_matrix.md`
- `task13_guard_extractor_design.md`
- `task13_service_aggregation_plan.md`

## Task 14 详细说明：制定搜索链路统一与性能优化方案

### 任务描述
- 目标：统一当前搜索实现路径，建立查询 DSL、provider 抽象与底层实现的清晰分层，并定义性能基线。
- 范围：覆盖全局搜索、房间内搜索、索引初始化、查询参数归一化和潜在外部搜索后端扩展点。
- 核心产出：搜索架构方案、查询 DSL、provider 接口、Postgres FTS 策略、性能优化路线图与迁移计划。

### 执行步骤
1. 盘点现有搜索入口、旁路 SQL、索引依赖、排序/分页/过滤行为及返回结构差异。
2. 设计统一查询 DSL，明确关键词、分页、过滤、排序、highlight、范围约束等字段语义。
3. 定义 provider 抽象，至少覆盖 Postgres FTS，并预留 Elasticsearch 等外部实现接口。
4. 制定索引策略与字段规范化方案，识别需要淘汰的低效 `LIKE` 兜底路径。
5. 输出分阶段迁移计划，说明兼容策略、性能验证方法和回退条件。

### 技术要求
- 方案必须兼容现有 Matrix/Synapse 搜索语义，不得仅按内部实现方便性设计。
- DSL 和 provider 需要清晰区分请求层、领域层、底层执行层。
- 性能设计必须包含索引命中、分页稳定性、排序一致性与高亮生成策略。
- 必须说明多后端场景下的能力降级和错误处理语义。

### 完成时限
- 第一阶段（2 个工作日）：完成现状盘点与问题分类。
- 第二阶段（2 个工作日）：完成 DSL、provider、索引与性能策略设计。
- 第三阶段（1 个工作日）：完成迁移方案、风险说明与评审稿。

### 质量指标
- 搜索入口覆盖率达到 100%，不存在未纳入统一方案的主路径。
- 方案必须消除或标记所有长期保留的低效旁路查询。
- 至少给出一组可量化性能目标，如响应时间、索引命中率或分页稳定性指标。
- 兼容层与执行层的边界清晰，不允许 DSL 直接泄漏底层实现细节。

### 测试验证方法
- 语义验证：为 DSL 定义等价查询样例，验证行为一致性。
- 性能验证：设计基准测试或 `EXPLAIN ANALYZE` 样例，验证索引策略合理。
- 兼容验证：列出回归测试集合，覆盖分页、排序、过滤与高亮。
- 实施验证：给出 provider 切换或降级时的 smoke test 清单。

### 验收标准
- 已形成统一搜索链路架构图与接口定义。
- 已明确 Postgres FTS 为最小落地实现，并预留外部 provider 扩展位。
- 已形成性能治理目标、迁移顺序与失败回滚条件。
- 已建立可执行的测试与基准验证要求。

### 预期交付物
- `task14_search_architecture_plan.md`
- `task14_search_dsl_and_provider.md`
- `task14_search_performance_baseline.md`

## Task 15 详细说明：制定 schema contract test 与 migration gate 方案

### 任务描述
- 目标：建立“迁移文件 -> 实际 schema -> SQL 查询 -> 集成测试”的闭环校验链，阻断 schema 漂移。
- 范围：优先覆盖 E2EE、account_data、space、search 等强依赖数据库结构的模块，并兼顾通用迁移门禁。
- 核心产出：关键表清单、契约测试策略、migration gate 设计、失败分类标准与 CI 接线建议。

### 执行步骤
1. 识别关键表、关键字段、关键索引和高风险 SQL 查询，建立 schema 依赖清单。
2. 为每个能力域定义最小 schema contract test，覆盖字段存在性、类型、默认值、可空性与查询语义。
3. 设计 migration gate 执行链路，明确初始化数据库、应用迁移、运行校验、执行测试的顺序。
4. 建立失败分类模型，区分迁移缺失、查询错误、领域映射错误、测试数据问题。
5. 输出 CI 接线建议，包括阻断条件、日志格式、失败产物与本地复现流程。

### 技术要求
- 所有关键字段命名与类型必须对齐项目既有数据库规范，如 `_ts`、`_at`、`is_` 前缀约束。
- 契约测试必须以真实数据库为基础，不允许完全依赖 mock 替代。
- migration gate 必须支持本地复现与 CI 自动执行，输出应可定位到具体迁移或查询。
- 需要兼顾 SQLx 查询、手写 SQL 与存储层抽象，不得只检查 migration 文件存在性。

### 完成时限
- 第一阶段（2 个工作日）：完成关键表与查询清单梳理。
- 第二阶段（2 个工作日）：完成 contract test 与 migration gate 设计。
- 第三阶段（1 个工作日）：完成 CI 接线建议、失败分类与实施说明。

### 质量指标
- 关键能力域覆盖率不低于 80%，P0/P1 模块必须全部纳入。
- 每类失败都必须具备可定位、可复现、可归因的输出格式。
- 至少形成一条端到端闭环样例，证明迁移与查询可以被同一门禁校验。
- 不允许出现“文档有规范但 gate 无检查项”的空转设计。

### 测试验证方法
- Schema 验证：运行迁移后检查表、列、索引、默认值与可空性。
- 查询验证：执行关键 SQL/SQLx 查询并断言返回结构与字段映射。
- 集成验证：以真实样例覆盖写入、读取、更新、错误分支。
- 门禁验证：模拟一条字段缺失或命名漂移场景，确认 gate 能阻断并输出可读错误。

### 验收标准
- 已形成关键表/关键查询矩阵与优先级。
- 已形成 migration gate 设计、失败分类与 CI 接线建议。
- 已形成可执行的 schema contract test 基线，不依赖人工心证。
- 已说明本地复现、CI 执行与问题定位路径。

### 预期交付物
- `task15_schema_dependency_inventory.md`
- `task15_schema_contract_test_plan.md`
- `task15_migration_gate_design.md`

## Task 16 详细说明：制定测试体系与工作区产物治理方案

### 任务描述
- 目标：建立覆盖占位接口探测、schema 回归、路由契约测试的测试基线，并规范测试文件、文档、产物与临时报表的工作区治理方式。
- 范围：覆盖 `tests/` 目录组织、CI 产物策略、`docs`/`artifacts`/`test-results` 分层与长期治理规则。
- 核心产出：测试基线设计、目录与命名规范、产物治理规则、CI 产物保存策略与评审边界说明。

### 执行步骤
1. 基于 Task 7、Task 11、Task 15 的结果，定义三类核心测试基线：占位接口探测、schema 回归、路由契约测试。
2. 盘点现有超大测试文件与分散产物，制定按能力域拆分的目录与命名规范。
3. 划分源码必审、文档必审、可生成报告、临时产物四类工作区内容，并定义存放位置与生命周期。
4. 制定 CI 产物策略，明确哪些测试结果应持久化、哪些应只在失败时上传、哪些应禁止进入主干。
5. 输出团队执行约定，包括新测试归属、临时报表归档、评审期望与清理规则。

### 技术要求
- 测试基线必须强调“真实业务数据断言”，禁止仅验证状态码或空响应。
- 目录规范必须兼容 Rust workspace 与当前 `tests/` 组织方式，避免大规模无收益搬迁。
- 产物治理规则必须区分长期文档、阶段性报告、自动生成文件和一次性调试材料。
- CI 策略必须说明保存期限、命名规范与失败时的诊断价值。

### 完成时限
- 第一阶段（2 个工作日）：完成测试基线与现状问题盘点。
- 第二阶段（2 个工作日）：完成目录规范、产物分层与 CI 策略设计。
- 第三阶段（1 个工作日）：完成团队执行约定与验收稿。

### 质量指标
- 三类测试基线都必须有明确适用场景、最小样例和接线入口。
- 测试文件拆分规则必须可操作，避免“按需自定”导致继续膨胀。
- 工作区产物分类覆盖率达到 100%，常见文档/产物均能归类。
- 规则需显著降低评审噪音，不允许继续把临时报告长期混入主干。

### 测试验证方法
- 组织验证：抽样检查现有测试文件，确认能按新规则归类。
- 产物验证：抽样检查 `docs`、`artifacts`、`test-results`、临时 Markdown 报表的分类是否合理。
- CI 验证：设计一次失败场景，确认产物上传与命名策略满足排障需求。
- 回归验证：确认新基线与 Task 11/15 设计保持一致，不出现重复或冲突定义。

### 验收标准
- 已形成三类测试基线及其最小执行入口。
- 已形成测试目录与命名规范，可直接指导后续新文件落位。
- 已形成工作区产物分层治理规则与 CI 产物策略。
- 已形成面向评审和日常维护的团队执行约定。

### 预期交付物
- `task16_test_baseline_plan.md`
- `task16_test_organization_rules.md`
- `task16_workspace_artifact_governance.md`
