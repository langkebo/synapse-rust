# 数据库系统性优化规范

## 一、项目概述

### 1.1 项目背景
项目 `synapse-rust` 是一个Matrix协议的Rust实现，在前期测试过程中发现了大量数据库Schema与代码定义不一致的问题，需要进行系统性优化。

### 1.2 优化目标
1. ✅ 完善数据库设计规范文档
2. ✅ 修复所有数据库结构问题
3. ✅ 优化迁移脚本
4. ✅ 修复代码与数据库不一致问题

### 1.3 当前问题汇总 ✅ 已修复

#### 已发现的数据库问题
| 表名 | 问题类型 | 修复状态 |
|------|----------|----------|
| users | 字段缺失 | ✅ 已修复 |
| devices | 字段缺失 | ✅ 已修复 |
| refresh_tokens | 字段缺失 | ✅ 已修复 |
| access_tokens | 字段缺失 | ✅ 已修复 |
| token_blacklist | 字段命名 | ✅ 已修复 |
| user_threepids | 字段缺失 | ✅ 已修复 |
| rooms | 字段重复 | ✅ 已修复 |
| events | 字段缺失 | ✅ 已修复 |
| e2ee_key_requests | 表缺失 | ✅ 已创建 |
| e2ee_secret_storage_keys | 表缺失 | ✅ 已创建 |
| federation_signing_keys | 表缺失 | ✅ 已创建 |

## 二、优化范围

### 2.1 文档优化 ✅ 完成
- [x] 审查并完善 `migrations/DATABASE_FIELD_STANDARDS.md`
- [x] 添加所有表的完整定义
- [x] 明确字段命名规范

### 2.2 数据库优化 ✅ 完成
- [x] 创建缺失的表
- [x] 添加缺失的字段
- [x] 修复字段命名不一致
- [x] 添加必要的索引

### 2.3 脚本优化 ✅ 完成
- [x] 整合迁移脚本
- [x] 删除冗余脚本
- [x] 优化脚本执行顺序

### 2.4 代码修复 ✅ 完成
- [x] 修复所有SQL查询中的字段名不匹配
- [x] 确保代码与数据库结构一致

## 三、预期成果

1. ✅ 完整的数据库设计规范文档
2. ✅ 一致的数据库结构
3. ✅ 高效的迁移脚本
4. ✅ 稳定的应用代码（编译成功）

## 四、修改文件清单

### 4.1 新增文件
- `migrations/20260303000003_comprehensive_schema_fix.sql` - 综合修复脚本
- `spec.md` - 优化规范文档
- `tasks.md` - 任务列表
- `checklist.md` - 检查清单

### 4.2 修改文件
- `src/storage/event.rs` - 修复字段映射
- `src/storage/monitoring.rs` - 修复查询
- `src/federation/key_rotation.rs` - 修复类型问题

## 五、下一步操作

1. 🔄 构建Docker镜像（进行中）
2. ⏳ 重启服务
3. ⏳ 执行API测试验证
