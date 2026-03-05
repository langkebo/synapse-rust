# 数据库全面审计与一致性验证 Spec

## Why
项目经过数据库优化后，需要全面排查数据库与代码之间的一致性问题，确保优化措施未对现有功能产生负面影响，所有数据交互操作仍能正确执行。

## What Changes
- 全面检查数据库表结构设计与代码模型定义的一致性
- 核查所有列定义是否完整、准确，数据类型是否恰当
- 验证字段约束是否正确设置并有效执行
- 检查时间戳字段命名规范（*_ts）的统一性
- 验证布尔字段命名规范（is_*）的一致性
- 确保数据库结构与代码实现之间无定义冲突

## Impact
- Affected specs: 数据库架构、代码模型定义
- Affected code: 
  - src/storage/*.rs (所有存储层)
  - src/e2ee/*/models.rs (E2EE 模型)
  - src/auth/mod.rs (认证模块)
  - src/services/*.rs (服务层)
  - migrations/00000000_unified_schema_v4.sql

## ADDED Requirements

### Requirement: 数据库表结构一致性
系统 SHALL 确保所有数据库表结构与 Rust 代码中的结构体定义保持一致。

#### Scenario: 表结构验证
- **WHEN** 检查数据库表结构
- **THEN** 每个表都应有对应的 Rust 结构体
- **AND** 结构体字段名应与表列名匹配
- **AND** 数据类型应兼容

### Requirement: 时间戳字段命名规范
系统 SHALL 统一使用 `*_ts` 后缀的时间戳字段命名。

#### Scenario: 时间戳字段检查
- **WHEN** 检查时间戳字段
- **THEN** 所有创建时间字段应为 `created_ts`
- **AND** 所有更新时间字段应为 `updated_ts`
- **AND** 所有过期时间字段应为 `expires_ts`（业务逻辑字段除外）
- **AND** 所有最后使用时间字段应为 `last_used_ts`

### Requirement: 布尔字段命名规范
系统 SHALL 统一使用 `is_*` 前缀的布尔字段命名。

#### Scenario: 布尔字段检查
- **WHEN** 检查布尔字段
- **THEN** 所有布尔字段应使用 `is_*` 前缀
- **AND** API 响应可使用 `#[serde(rename)]` 保持兼容性

### Requirement: 字段约束一致性
系统 SHALL 确保数据库约束与代码验证逻辑一致。

#### Scenario: 约束验证
- **WHEN** 检查字段约束
- **THEN** NOT NULL 约束应与代码中 Option 类型对应
- **AND** UNIQUE 约束应在代码中有相应验证
- **AND** FOREIGN KEY 约束应正确关联

### Requirement: SQL 查询字段匹配
系统 SHALL 确保所有 SQL 查询使用正确的字段名。

#### Scenario: SQL 查询验证
- **WHEN** 执行 SQL 查询
- **THEN** SELECT 语句中的字段名应与表结构匹配
- **AND** INSERT 语句中的字段名应正确
- **AND** UPDATE 语句中的字段名应正确

## MODIFIED Requirements
无

## REMOVED Requirements
无
