# 数据库脚本优化检查清单

## 一、迁移前检查

### 1.1 环境准备
- [ ] 确认 Docker 服务正常运行
- [ ] 确认数据库容器正常运行
- [ ] 确认 Redis 容器正常运行
- [ ] 备份现有数据库数据

### 1.2 问题识别
- [ ] 列出所有迁移脚本文件
- [ ] 分析每个脚本的表定义
- [ ] 识别列名冲突问题
- [ ] 识别缺失的表
- [ ] 识别缺失的列
- [ ] 识别执行顺序问题

## 二、迁移脚本检查

### 2.1 核心表脚本
- [ ] users 表定义正确
- [ ] devices 表定义正确
- [ ] access_tokens 表定义正确
- [ ] refresh_tokens 表定义正确
- [ ] 外键约束正确

### 2.2 房间和成员表脚本
- [ ] rooms 表定义正确
- [ ] room_memberships 表定义正确
- [ ] room_members 表定义正确 (新增)
- [ ] room_aliases 表定义正确
- [ ] 外键约束正确

### 2.3 事件表脚本
- [ ] events 表定义正确
- [ ] events.type 列存在
- [ ] event_contents 表定义正确
- [ ] event_signatures 表定义正确

### 2.4 E2EE密钥表脚本
- [ ] device_keys 表定义正确
- [ ] cross_signing_keys 表定义正确
- [ ] megolm_sessions 表定义正确
- [ ] 外键约束正确

### 2.5 媒体和语音表脚本
- [ ] media_repository 表定义正确
- [ ] voice_messages 表定义正确
- [ ] voice_messages.processed_ts 列存在
- [ ] voice_messages.mime_type 列存在
- [ ] voice_messages.encryption 列存在

### 2.6 推送通知表脚本
- [ ] pushers 表定义正确 (新增)
- [ ] push_rules 表定义正确 (新增)
- [ ] 默认推送规则数据已插入

### 2.7 扩展功能表脚本
- [ ] spaces 表定义正确
- [ ] threads 表定义正确
- [ ] application_services 表定义正确

### 2.8 索引脚本
- [ ] 所有索引在表创建后执行
- [ ] 索引名称唯一
- [ ] 索引类型正确

## 三、迁移执行检查

### 3.1 执行前
- [ ] 数据库卷已删除 (全新初始化)
- [ ] 旧容器已停止
- [ ] 新镜像已构建

### 3.2 执行中
- [ ] 迁移脚本按顺序执行
- [ ] 无 SQL 语法错误
- [ ] 无外键约束错误
- [ ] 无重复定义警告

### 3.3 执行后
- [ ] 所有表创建成功
- [ ] 所有索引创建成功
- [ ] 默认数据插入成功
- [ ] 日志无错误和警告

## 四、服务启动检查

### 4.1 容器状态
- [ ] synapse-postgres 容器健康
- [ ] synapse-redis 容器健康
- [ ] synapse-rust 容器健康
- [ ] synapse-nginx 容器健康

### 4.2 服务健康检查
- [ ] `/_matrix/federation/v1/version` 返回 200
- [ ] 服务日志无错误
- [ ] 服务日志无警告

## 五、API测试检查

### 5.1 语音消息API
- [ ] 上传语音消息返回 200
- [ ] 获取语音消息返回 200
- [ ] 获取用户语音消息返回 200
- [ ] 获取房间语音消息返回 200
- [ ] 删除语音消息返回正确状态码

### 5.2 推送通知API
- [ ] 获取推送器列表返回 200
- [ ] 设置推送器返回 200
- [ ] 获取推送规则返回 200
- [ ] 设置推送规则返回 200
- [ ] 删除推送规则返回正确状态码

### 5.3 搜索API
- [ ] 搜索功能返回 200
- [ ] 获取房间线程返回 200
- [ ] 获取房间层级返回 200
- [ ] 时间戳转事件返回 200

### 5.4 媒体管理API
- [ ] 上传媒体文件返回 200
- [ ] 下载媒体文件返回正确状态码
- [ ] 下载不存在媒体返回 404
- [ ] 删除媒体文件返回正确状态码

## 六、数据完整性检查

### 6.1 表结构验证
```sql
-- 验证 voice_messages 表
SELECT column_name FROM information_schema.columns 
WHERE table_name = 'voice_messages' 
AND column_name IN ('processed_ts', 'mime_type', 'encryption');

-- 验证 pushers 表
SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'pushers';

-- 验证 push_rules 表
SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'push_rules';

-- 验证 room_members 表
SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'room_members';
```

- [ ] voice_messages 表列完整
- [ ] pushers 表存在
- [ ] push_rules 表存在
- [ ] room_members 表存在

### 6.2 默认数据验证
```sql
-- 验证默认推送规则
SELECT COUNT(*) FROM push_rules WHERE user_id = '.default';
```

- [ ] 默认推送规则存在

## 七、文档更新检查

- [ ] api-error.md 问题状态已更新
- [ ] 数据库架构文档已更新
- [ ] 迁移脚本说明已更新

## 八、回滚准备检查

- [ ] 数据库备份文件已创建
- [ ] 旧镜像已保存
- [ ] 回滚脚本已准备
- [ ] 回滚步骤已文档化

## 检查结果汇总

| 检查类别 | 通过项 | 失败项 | 通过率 |
|----------|--------|--------|--------|
| 迁移前检查 | - | - | - |
| 迁移脚本检查 | - | - | - |
| 迁移执行检查 | - | - | - |
| 服务启动检查 | - | - | - |
| API测试检查 | - | - | - |
| 数据完整性检查 | - | - | - |
| 文档更新检查 | - | - | - |
| 回滚准备检查 | - | - | - |

**总体评估**: 待执行

**签字确认**: ________________

**日期**: ________________
