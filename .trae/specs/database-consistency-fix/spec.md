# 数据库表结构与Rust代码一致性修复规范

## Why

项目中存在数据库表结构与Rust代码定义不一致的问题，导致运行时错误和潜在的数据完整性问题。需要全面检查并修复所有不匹配问题，确保代码质量和系统稳定性。

## What Changes

### 数据库Schema修复
- **BREAKING**: 修复 `users` 表字段命名：`admin` -> `is_admin`, `deactivated` -> `is_deactivated`, `shadow_banned` -> `is_shadow_banned`
- **BREAKING**: 修复 `access_tokens` 表字段：`invalidated_ts` -> `revoked_ts`, `expires_ts` 改为可空
- **BREAKING**: 修复 `refresh_tokens` 表字段：`token` -> `token_hash`, `expires_ts` -> `expires_at`, `invalidated` -> `is_revoked`
- **BREAKING**: 修复 `devices` 表：确保使用 `created_ts` 而非 `created_at`
- 统一所有时间戳字段使用 BIGINT 类型（毫秒级）
- 统一所有布尔字段使用 `is_` 前缀

### Rust代码修复
- 修复 `User` 结构体字段类型和命名
- 修复 `AccessToken` 结构体字段类型
- 修复 `RefreshToken` 结构体字段命名
- 修复 `Device` 结构体字段命名
- 更新所有SQL查询语句以匹配修复后的字段名

### 配置文件优化
- 合并 `homeserver.local.yaml` 和 `homeserver.yaml` 配置文件
- 优化数据库连接池配置
- 统一配置格式和注释

### Docker镜像重构
- 清理构建缓存
- 删除旧镜像
- 重新构建并运行项目

## Impact

- Affected specs: 数据库迁移系统、存储层API
- Affected code: 
  - `src/storage/user.rs`
  - `src/storage/device.rs`
  - `src/storage/token.rs`
  - `src/storage/refresh_token.rs`
  - `src/storage/room.rs`
  - `src/storage/event.rs`
  - `src/storage/membership.rs`
  - `migrations/*.sql`
  - `docker/config/homeserver*.yaml`

## ADDED Requirements

### Requirement: 数据库字段命名一致性

系统应确保所有数据库表字段遵循以下命名规范：

#### Scenario: 布尔字段命名
- **WHEN** 定义布尔类型字段
- **THEN** 必须使用 `is_` 或 `has_` 前缀（如 `is_admin`, `is_revoked`）

#### Scenario: 时间戳字段命名
- **WHEN** 定义时间戳字段
- **THEN** NOT NULL 时间戳使用 `_ts` 后缀（如 `created_ts`）
- **AND** 可空时间戳使用 `_at` 后缀（如 `expires_at`, `revoked_at`）

### Requirement: Rust结构体与数据库字段匹配

系统应确保Rust结构体字段与数据库表字段完全匹配：

#### Scenario: 字段类型匹配
- **WHEN** 数据库字段为 NOT NULL
- **THEN** Rust结构体字段使用基本类型（如 `i64`, `bool`）
- **WHEN** 数据库字段可为 NULL
- **THEN** Rust结构体字段使用 `Option<T>` 类型

#### Scenario: 字段命名匹配
- **WHEN** 数据库字段名为 `is_xxx`
- **THEN** Rust结构体字段名应为 `is_xxx`（直接映射）
- **WHEN** 需要使用不同名称时
- **THEN** 必须使用 `#[sqlx(rename = "db_field_name")]` 属性

### Requirement: 数据库迁移版本控制

系统应提供清晰的迁移版本控制：

#### Scenario: 迁移文件命名
- **WHEN** 创建新的迁移文件
- **THEN** 文件名格式为 `YYYYMMDDHHMMSS_description.sql`

#### Scenario: 迁移执行
- **WHEN** 执行迁移
- **THEN** 必须记录版本号、执行时间和校验和
- **AND** 支持幂等执行

## MODIFIED Requirements

### Requirement: 配置文件管理

系统配置文件应合并优化：

- 合并 `homeserver.local.yaml` 和 `homeserver.yaml` 为单一配置文件
- 使用环境变量覆盖敏感配置
- 保留开发环境和生产环境的配置差异

## REMOVED Requirements

### Requirement: 冗余字段定义

**Reason**: 避免数据不一致和混淆
**Migration**: 
- 删除 `invalidated` 字段，使用 `is_revoked`
- 删除 `invalidated_ts` 字段，使用 `revoked_ts`
- 删除 `created_at` 字段，使用 `created_ts`
