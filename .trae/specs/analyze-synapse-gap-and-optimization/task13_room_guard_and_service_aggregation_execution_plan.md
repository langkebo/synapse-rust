# Task 13 - 统一权限守卫与服务聚合执行文档

## 1. 目标

统一房间相关接口中的存在性、成员身份、管理员权限等重复校验，形成可复用的 guard/extractor 体系，并为 `ServiceContainer` 向领域服务聚合演进提供阶段化方案。

## 2. 范围

- 路由范围：房间主链、space、threads、E2EE 房间相关路由
- 共享逻辑：`room_exists`、`is_member`、`is_admin`、owner/creator 校验
- 架构范围：`ServiceContainer` 到 `RoomServices`、`E2eeServices` 等聚合方向
- 非目标：本任务不直接完成全量代码替换

## 3. 输入

- `tasks.md` 中 `Task 13 详细说明`
- `spec.md` 中“统一房间访问守卫”要求
- `Task 12` 拆分方案
- 现有房间权限相关路由实现

## 4. 输出

- 重复权限校验矩阵
- guard/extractor 设计草案
- 服务聚合演进路径和试点范围

## 5. guard 模型候选

- `RoomMustExist`
- `RoomMemberOnly`
- `RoomMemberOrAdmin`
- `RoomOwnerOnly`
- `RoomStateEditor`
- `RoomAdminOnly`

## 6. 执行阶段

### Phase 1: 重复模式盘点

- 统计高频权限校验组合
- 记录每类组合的成功、拒绝、对象不存在语义
- 标记日志字段、错误码和数据库访问模式

### Phase 2: guard/extractor 设计

- 为每个 guard 定义输入、输出、错误语义和审计字段
- 区分适合用 extractor 还是 helper 的场景
- 明确 guard 的可组合性和依赖顺序

### Phase 3: 服务聚合设计

- 识别 `ServiceContainer` 中与房间域强相关的依赖簇
- 输出 `RoomServices`、`E2eeServices` 等聚合接口草案
- 定义构造方式、生命周期和测试替身策略

### Phase 4: 迁移计划

- 选择首批试点接口
- 定义迁移步骤、兼容要求和回归测试集合
- 输出长期替换路线

## 7. 技术约束

- guard 不能显著增加热路径数据库往返
- 同类权限场景必须统一 403/404 语义
- 服务聚合不能牺牲测试注入能力
- 审计字段必须可追踪 `user_id`、`room_id`、校验类型和拒绝原因

## 8. 里程碑与时限

- D1-D2：完成重复校验矩阵与 guard 分类
- D3-D4：完成 guard/extractor 设计和服务聚合草案
- D5：完成试点范围与迁移计划定稿

## 9. 质量指标

- 高频权限场景覆盖率达到 90% 以上
- 同类权限场景只保留一套错误语义
- 首批试点接口清晰可执行
- 聚合方案能降低服务构造复杂度

## 10. 测试与验证

- 样例验证：至少抽取 5 组典型权限路径
- 语义验证：每个 guard 都有成功/拒绝/不存在样例
- 集成验证：定义迁移后所需权限测试集合
- 架构验证：评估聚合后构造与测试注入复杂度

## 11. 风险与缓解

- 风险：guard 过细导致使用复杂
- 缓解：优先提炼高频场景，避免一开始定义过多变体

- 风险：错误语义统一时与既有行为冲突
- 缓解：保留迁移前后对照矩阵，逐步替换

- 风险：服务聚合后隐藏过多依赖
- 缓解：聚合接口中显式列出子服务职责边界

## 12. 验收标准

- 已形成统一 guard/extractor 模型
- 已形成权限错误语义规范
- 已形成 `ServiceContainer` 向领域聚合演进的阶段性方案
- 已明确首批试点接口和回归验证要求

## 13. 后续关联交付物

- `task13_room_guard_matrix.md`
- `task13_guard_extractor_design.md`
- `task13_service_aggregation_plan.md`
