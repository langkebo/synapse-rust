# Tasks

- [x] Task 1: 建立数据库审查基线
  - [x] SubTask 1.1: 盘点所有 Schema 与迁移来源
  - [x] SubTask 1.2: 提取运行时 SQL 与模型定义
  - [x] SubTask 1.3: 生成对象与字段映射矩阵
  - [x] SubTask 1.4: 固化审查输入数据快照

- [x] Task 2: 执行架构完整性审查
  - [x] SubTask 2.1: 校验表与视图存在性和可重建性
  - [x] SubTask 2.2: 校验主键与唯一约束完整性
  - [x] SubTask 2.3: 校验外键关系与级联策略
  - [x] SubTask 2.4: 校验检查约束的业务有效性

- [x] Task 3: 执行字段规范性审查
  - [x] SubTask 3.1: 审查字段命名规范与历史兼容命名
  - [x] SubTask 3.2: 审查字段类型与 Rust 映射一致性
  - [x] SubTask 3.3: 审查默认值、NULL 语义与 NOT NULL 约束
  - [x] SubTask 3.4: 审查索引缺失与冗余索引问题

- [x] Task 4: 识别数据库设计缺陷
  - [x] SubTask 4.1: 识别冗余字段与重复存储
  - [x] SubTask 4.2: 评估核心表第三范式符合性
  - [x] SubTask 4.3: 识别可能导致不一致的结构设计
  - [x] SubTask 4.4: 识别审计字段缺失并制定补齐策略
  - [x] Progress Note: 已识别重复索引、room_summary 关联约束缺失及审计字段覆盖不完整问题

- [x] Task 5: 输出分级审查报告与重构优先级
  - [x] SubTask 5.1: 汇总问题证据并按严重度分级
  - [x] SubTask 5.2: 为每项问题输出修复方案与风险说明
  - [x] SubTask 5.3: 制定分阶段重构优先级路线图
  - [x] SubTask 5.4: 输出数据库性能优化建议清单

- [x] Task 6: 在开发环境实施高优先级修复
  - [x] SubTask 6.1: 编写并应用迁移脚本与代码修复
  - [x] SubTask 6.2: 执行迁移重放与回滚验证
  - [x] SubTask 6.3: 更新相关模型与查询以保持一致
  - [x] SubTask 6.4: 记录修复影响面与兼容策略
  - [x] Progress Note: 已新增 20260304000003 优化迁移（重复索引清理、room_summary 约束补齐、审计字段补齐）
  - [x] Progress Note: 已新增 20260304000004 收敛字段规范（移除 created_at 警告来源并补齐关键 NOT NULL）

- [x] Task 7: 完成验证与基准测试
  - [x] SubTask 7.1: 执行数据完整性测试套件
  - [x] SubTask 7.2: 执行核心业务数据库回归测试
  - [x] SubTask 7.3: 执行性能基准前后对比
  - [x] SubTask 7.4: 形成最终验证结论与遗留问题清单
  - [x] Progress Note: 核心回归脚本当前 6/6 通过（stats 已提升为 200）
  - [x] Progress Note: 数据完整性套件通过（db_migrate validate、verify_migration、schema_validator、monitoring）
  - [x] Progress Note: database_bench 前后对比已完成（主要指标波动在噪声范围内）

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 1]
- [Task 4] depends on [Task 1]
- [Task 5] depends on [Task 2, Task 3, Task 4]
- [Task 6] depends on [Task 5]
- [Task 7] depends on [Task 6]

# Milestones
- M1: 审查基线完成（Task 1）
- M2: 问题清单与分级完成（Task 2-5）
- M3: 高优先级修复完成（Task 6）
- M4: 验证闭环完成（Task 7）
