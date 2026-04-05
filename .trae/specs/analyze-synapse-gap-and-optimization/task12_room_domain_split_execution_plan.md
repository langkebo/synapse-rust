# Task 12 - 房间域与核心路由拆分执行文档

## 1. 目标

将 `room.rs`、`middleware.rs`、`e2ee_routes.rs` 的职责按领域拆分为可维护模块，降低单文件复杂度，收敛共享依赖，并保持现有路由语义、错误语义和测试入口不回退。

## 2. 范围

- 主目标文件：`src/web/routes/handlers/room.rs`
- 关联文件：`src/web/routes/middleware.rs`、`src/web/routes/e2ee_routes.rs`
- 关联装配：`src/web/routes/assembly.rs`
- 非目标：本任务不直接改造业务实现逻辑，不以新增功能为目标

## 3. 输入

- `tasks.md` 中 `Task 12 详细说明`
- `spec.md` 中“建立房间域模块拆分方案”要求
- `task11_room_rs_placeholder_inventory.md`
- `task11_other_routes_placeholder_inventory.md`

## 4. 输出

- 模块拆分蓝图
- 旧函数到新模块的迁移矩阵
- 迁移顺序、回归验证要求与回滚策略

## 5. 目标模块草案

- `room_account_data`
- `room_keys`
- `room_spaces`
- `room_threads`
- `room_render`
- `room_sync`
- `room_membership`
- `room_state`

## 6. 执行阶段

### Phase 1: 现状盘点

- 统计目标文件的函数簇、共享 helper、依赖服务与耦合点
- 标记横切能力：鉴权、日志、错误映射、参数校验、分页 token 处理
- 输出“不可直接拆分”的公共逻辑清单

### Phase 2: 边界设计

- 为每个候选模块定义职责、输入、输出、禁止跨界访问项
- 确定哪些逻辑保留在聚合层，哪些下沉到领域模块
- 为 `middleware.rs` 区分 guard/extractor 候选和保留中间件逻辑

### Phase 3: 迁移顺序设计

- 第一批：低耦合子域和纯 handler 抽取
- 第二批：中等耦合子域和共享辅助函数归位
- 第三批：聚合层瘦身和装配收口
- 为每一批定义回滚点和停止条件

### Phase 4: 验证与交付

- 输出迁移矩阵、验证清单和回滚方案
- 评审是否满足“路由不回退、测试不失联、错误语义不漂移”

## 7. 技术约束

- 必须按 bounded context 拆分，不能仅按行数平均切分
- 不能改变外部路由路径、HTTP 方法和错误码语义
- 不能引入循环依赖
- 聚合入口应尽量保持在现有装配结构内，避免一次性重构 `assembly.rs`

## 8. 里程碑与时限

- D1-D2：完成现状盘点与候选模块划分
- D3-D4：完成迁移矩阵、边界说明与回滚策略
- D5：完成评审稿与实施建议定稿

## 9. 质量指标

- 候选模块覆盖 `room.rs` 主要职责 90% 以上
- 每个模块都具备清晰职责与依赖说明
- 迁移矩阵覆盖关键 handler，不留“大量未归类”
- 回归验证清单覆盖主路由、错误语义和测试入口

## 10. 测试与验证

- 结构验证：检查旧函数到新模块映射是否唯一
- 路由验证：抽样验证关键端点 URL 与方法不变
- 依赖验证：检查服务依赖是否收敛、无新增跨域耦合
- 回归验证：列出拆分后必须执行的单元测试和集成测试清单

## 11. 风险与缓解

- 风险：模块拆分后共享逻辑散落
- 缓解：先识别公共 helper，再决定下沉或保留

- 风险：错误语义在迁移中漂移
- 缓解：为高频错误路径建立迁移前后对照表

- 风险：测试入口丢失
- 缓解：在迁移矩阵中同步记录测试影响面

## 12. 验收标准

- 已形成可执行的模块拆分蓝图
- 已明确迁移顺序、停止条件和回滚方案
- 已给出拆分后的回归验证要求
- 已明确 `room.rs`、`middleware.rs`、`e2ee_routes.rs` 的职责边界收敛路径

## 13. 后续关联交付物

- `task12_route_migration_matrix.md`
- `task12_validation_and_rollback.md`
