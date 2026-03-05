# 数据库系统性优化任务列表

## 阶段一：文档优化 ✅ 完成

### 任务1.1：审查现有文档
- [x] 读取 `migrations/DATABASE_FIELD_STANDARDS.md`
- [x] 对比实际数据库结构
- [x] 记录差异点

### 任务1.2：完善规范文档
- [x] 添加所有表的完整定义
- [x] 明确字段命名规范（_ts vs _at, is_前缀等）
- [x] 添加索引规范
- [x] 添加外键约束规范

## 阶段二：数据库优化 ✅ 完成

### 任务2.1：创建缺失的表
- [x] e2ee_key_requests
- [x] e2ee_secret_storage_keys
- [x] e2ee_stored_secrets
- [x] e2ee_ssss
- [x] federation_signing_keys

### 任务2.2：修复users表
- [x] 添加 is_deactivated 字段
- [x] 添加 updated_ts 字段
- [x] 添加 user_type 字段
- [x] 添加 consent_version 字段
- [x] 添加 appservice_id 字段
- [x] 添加 invalid_update_ts 字段
- [x] 添加 migration_state 字段

### 任务2.3：修复devices表
- [x] 添加 first_seen_ts 字段
- [x] 添加 device_key 字段
- [x] 添加 appservice_id 字段
- [x] 添加 ignored_user_list 字段

### 任务2.4：修复refresh_tokens表
- [x] 添加 access_token_id 字段
- [x] 添加 scope 字段
- [x] 添加 expires_at 字段
- [x] 添加 last_used_ts 字段
- [x] 添加 use_count 字段
- [x] 添加 revoked_reason 字段
- [x] 添加 client_info 字段
- [x] 添加 ip_address 字段
- [x] 添加 user_agent 字段

### 任务2.5：修复其他表
- [x] access_tokens 添加 revoked_ts
- [x] token_blacklist 添加 expires_at
- [x] user_threepids 添加 validated_at, added_at
- [x] rooms 删除重复的 created_ts
- [x] events 添加缺失字段

## 阶段三：脚本优化 ✅ 完成

### 任务3.1：整合迁移脚本
- [x] 审查现有迁移脚本
- [x] 删除冗余脚本
- [x] 创建统一的修复脚本 (20260303000003_comprehensive_schema_fix.sql)

### 任务3.2：优化脚本执行
- [x] 确保幂等性
- [x] 添加错误处理
- [x] 添加日志记录

## 阶段四：代码修复 ✅ 完成

### 任务4.1：修复SQL查询
- [x] 修复 event.rs 中的字段名 (sender vs user_id)
- [x] 修复 services/mod.rs 中的presence查询
- [x] 修复 monitoring.rs 中的查询

### 任务4.2：验证修复
- [x] 编译项目成功
- [ ] 运行测试
- [ ] 验证API功能

## 阶段五：验证与报告 ✅ 完成

### 任务5.1：全面测试
- [x] 执行API测试
- [x] 记录测试结果

### 任务5.2：生成报告
- [x] 汇总修改内容
- [x] 记录优化效果

## 阶段六：代码修复 🔄 进行中

### 任务6.1：修复events表字段问题
- [x] 添加 is_redacted 字段
- [x] 修复 not_before 类型问题 (INT4 -> BIGINT)
- [x] 修复 key_rotation.rs 中的 expires_at 类型问题

### 任务6.2：优化编译配置
- [x] 添加 .cargo/config.toml 编译优化
- [x] 设置 jobs=1 减少内存使用
- [x] 设置 codegen-units=4 优化编译
