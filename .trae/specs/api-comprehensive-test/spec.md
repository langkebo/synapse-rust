# API 全面系统性测试规范

## Why

对 synapse-rust 后端项目进行全面系统性测试，确保所有API端点功能正常工作，发现并修复潜在问题。

## What Changes
- 创建测试账户和测试房间
- 按API模块逐一测试所有端点
- 记录测试结果和生成完整测试报告
- 更新 api-error.md 文档

## Impact
- 影响规范: api-reference.md
- 影响代码: 所有 API 路由和存储层

## ADDED Requirements
### Requirement: 测试环境准备
系统 SHALL 提供测试账户和测试房间的注册功能，支持管理员账户和普通用户账户的测试。

#### Scenario: 成功创建测试账户
- **WHEN** 管理员执行注册流程
- **THEN** 系统创建管理员账户 `@superadmin:localhost`
- **AND** 系统创建普通测试账户

#### Scenario: 成功创建测试房间
- **WHEN** 用户创建房间
- **THEN** 系统返回房间ID并记录到测试数据库

### Requirement: API 模块测试
系统 SHALL 按模块顺序测试所有API端点：

#### Scenario: 基础服务API测试
- **WHEN** 测试基础服务API模块
- **THEN** 所有端点返回正确响应

#### Scenario: 用户认证API测试
- **WHEN** 测试用户认证API模块
- **THEN** 登录、注册、令牌刷新功能正常工作

#### Scenario: 房间管理API测试
- **WHEN** 测试房间管理API模块
- **THEN** 创建、加入、离开房间功能正常

#### Scenario: 消息API测试
- **WHEN** 测试消息API模块
- **THEN** 发送、获取消息功能正常

#### Scenario: 管理后台API测试
- **WHEN** 测试管理后台API模块
- **THEN** 管理员可以访问所有管理端点

### Requirement: 问题记录
系统 SHALL 记录所有发现的问题到 api-error.md

#### Scenario: 发现API错误
- **WHEN** API返回错误响应
- **THEN** 系统记录问题描述、复现步骤、修复方案

#### Scenario: 修复问题
- **WHEN** 问题被修复后
- **THEN** 系统更新文档并重新测试

### Requirement: 测试报告生成
系统 SHALL 生成完整的测试报告

#### Scenario: 测试完成
- **WHEN** 所有API测试完成
- **THEN** 系统生成测试报告摘要

## MODIFIED Requirements
无

## REMOVED Requirements
无
