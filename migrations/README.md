# synapse-rust 数据库迁移

> 版本: 4.0.0
> 更新日期: 2026-03-01

## 概述

本目录包含 synapse-rust 项目的数据库架构定义和迁移管理工具。

## 目录结构

```
migrations/
├── 00000000_unified_schema_v4.sql  # 统一数据库架构 (v4.0.0)
├── DATABASE_FIELD_STANDARDS.md      # 字段命名规范
└── README.md                        # 本文档
```

## 快速开始

### 初始化数据库

```bash
# 设置环境变量
export DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse"

# 初始化数据库
./scripts/db_migrate.sh init
```

### 查看迁移状态

```bash
./scripts/db_migrate.sh status
```

### 验证数据库架构

```bash
./scripts/db_migrate.sh validate
```

## 架构版本

### v4.0.0 (2026-03-01)

统一数据库架构，包含以下改进：

1. **时间戳字段统一**
   - `created_at` → `created_ts` (BIGINT, 毫秒级时间戳)
   - `updated_at` → `updated_ts` (BIGINT, 毫秒级时间戳)
   - `expires_at` → `expires_ts` (BIGINT, 毫秒级时间戳)
   - `last_used_at` → `last_used_ts` (BIGINT, 毫秒级时间戳)

2. **布尔字段命名规范**
   - `admin` → `is_admin`
   - `enabled` → `is_enabled`
   - `valid` → `is_valid`
   - `suggested` → `is_suggested`

3. **性能优化**
   - 添加必要的索引
   - 优化外键约束
   - 统一使用 BIGINT 时间戳

## 字段命名规范

详见 [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md)

### 核心规则

| 类型 | 命名规则 | 示例 |
|------|----------|------|
| 时间戳 | `*_ts` 后缀 | `created_ts`, `updated_ts` |
| 布尔值 | `is_*` 前缀 | `is_admin`, `is_enabled` |
| 主键 | `id` | `id` |
| 外键 | `*_id` 后缀 | `user_id`, `room_id` |

## 迁移管理

### 使用脚本

```bash
# 初始化
./scripts/db_migrate.sh init

# 查看状态
./scripts/db_migrate.sh status

# 验证架构
./scripts/db_migrate.sh validate

# 帮助信息
./scripts/db_migrate.sh help
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DATABASE_URL` | - | 完整数据库连接字符串 |
| `DB_HOST` | localhost | 数据库主机 |
| `DB_PORT` | 5432 | 数据库端口 |
| `DB_NAME` | synapse | 数据库名称 |
| `DB_USER` | synapse | 数据库用户 |
| `DB_PASSWORD` | synapse | 数据库密码 |

## 数据库表概览

### 核心表

| 表名 | 说明 |
|------|------|
| `users` | 用户信息 |
| `devices` | 设备信息 |
| `access_tokens` | 访问令牌 |
| `refresh_tokens` | 刷新令牌 |
| `rooms` | 房间信息 |
| `events` | 事件信息 |

### E2EE 表

| 表名 | 说明 |
|------|------|
| `device_keys` | 设备密钥 |
| `cross_signing_keys` | 跨设备签名密钥 |
| `megolm_sessions` | Megolm 会话 |
| `olm_sessions` | Olm 会话 |

### 认证表

| 表名 | 说明 |
|------|------|
| `cas_tickets` | CAS 票据 |
| `saml_sessions` | SAML 会话 |
| `registration_captcha` | 注册验证码 |

## 注意事项

1. **备份数据**：执行迁移前请务必备份数据库
2. **测试环境**：建议先在测试环境验证迁移脚本
3. **版本兼容**：确保应用代码与数据库架构版本匹配

## 相关文档

- [项目规则](/.trae/rules/project_rules.md)
- [优化方案](/docs/spec/database-migration-optimization/optimization-plan.md)
- [完成报告](/docs/spec/database-migration-optimization/completion-report.md)
