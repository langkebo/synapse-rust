# API 端点系统性排查与优化 Spec

## Why
当前项目有 656 个 API 端点分布在 48 个模块中，需要系统性地排查每个端点的实现完整性、数据库表结构一致性，并修复发现的缺陷，确保所有端点功能完整且测试通过。

## What Changes
- 逐模块排查 API 端点实现完整性
- 检查代码与数据库表结构的字段一致性
- 修复发现的缺陷和缺失字段
- 补充缺失的 API 端点到 `api-complete.md`
- 编写/完善 API 测试文档
- 优化后进行测试验证

## Impact
- Affected specs: 所有 API 端点功能完整性
- Affected code: `src/web/routes/`, `src/storage/`, `src/services/`
- Affected database: 所有相关表结构

## ADDED Requirements

### Requirement: 系统性 API 排查
系统 SHALL 对所有 48 个模块进行系统性排查，每个模块包含：
1. API 端点实现检查
2. 数据库表结构验证
3. 字段一致性检查
4. 缺陷修复
5. 测试验证

#### Scenario: 模块排查流程
- **WHEN** 开始排查一个模块
- **THEN** 完成以下步骤：
  1. 列出该模块所有端点
  2. 检查每个端点的路由实现
  3. 验证对应的数据库表结构
  4. 记录发现的问题
  5. 修复所有问题
  6. 编写/更新测试
  7. 运行测试确保通过
  8. 更新 `api-complete.md`

### Requirement: 数据库表结构一致性
系统 SHALL 确保代码中的结构体字段与数据库表列完全匹配。

#### Scenario: 字段不匹配检测
- **WHEN** 发现代码结构体与数据库表字段不匹配
- **THEN** 采取以下措施之一：
  - 修改代码查询以匹配数据库列名
  - 添加缺失的数据库列（通过迁移）
  - 更新结构体定义以匹配数据库

### Requirement: 缺失端点补充
系统 SHALL 将发现但未记录的 API 端点补充到 `api-complete.md`。

#### Scenario: 发现新端点
- **WHEN** 在代码中发现未记录的 API 端点
- **THEN** 将端点添加到 `api-complete.md` 对应模块列表中

## MODIFIED Requirements

### Requirement: Space 模块优化
修复 Space 模块的数据库字段不匹配问题：
- `join_rule` vs `join_rules`
- `visibility` 字段缺失（从 `is_public` 派生）
- `room_id` 字段冗余（`space_id` 即为 `room_id`）

## REMOVED Requirements
无
